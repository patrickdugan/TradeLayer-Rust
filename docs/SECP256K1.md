# Public secp256k1 ECDSA verification

`secp256k1` implements real ECDSA verification of an already-computed 32-byte digest using native, fixed-width Rust arithmetic. It has no dependencies, unsafe code, private-key input, signing API, hashing choice, address binding, transaction admission or script interpreter. A verified signature proves the supplied digest/key/signature relation; the caller still has to authenticate the intended key and bind the complete intended payload.

## Public API and exact domain

```rust
verify_ecdsa_digest(
    digest: &[u8; 32], public_key: &[u8], compact_signature: &[u8; 64],
) -> Result<bool, SecpError>

verify_ecdsa_digest_strict(
    digest: &[u8; 32], public_key: &[u8], compact_signature: &[u8; 64],
) -> Result<bool, SecpError>

canonical_compressed_public_key(public_key: &[u8])
    -> Result<[u8; 33], SecpError>
```

Keys must use a 33-byte `02`/`03` compressed or 65-byte `04` uncompressed SEC1 encoding. Parsing requires canonical coordinates strictly below the field modulus and a finite point on the curve. Compressed parsing computes a square root, verifies that it exists, and selects/checks its stated parity. Infinity, x-only keys, hybrid encodings, wrong prefixes/lengths, nonresidues and noncanonical coordinates return `InvalidPublicKey`. The signature is compact big-endian `r || s`, with each scalar strictly between zero and the group order; zero or out-of-range scalars return `InvalidSignature`. No DER or recoverable-signature interpretation is implicit.

The default verifier accepts both low-S and mathematically equivalent high-S signatures. The strict verifier additionally requires `s <= floor(n/2)` and returns `Ok(false)` for a validly encoded high-S signature. High **R** is accepted in either mode. A well-formed cryptographic mismatch or an infinity verification sum returns `Ok(false)`. `Arithmetic` reserves an explicit error for an unavailable internal inverse; valid scalar parsing excludes zero before scalar inversion.

The independent reference library is genuine **tiny-secp256k1 2.2.4**. Its default/strict behavior agrees with the above high-S policies. Its README's statement about restricting both R and S is not its actual behavior: genuine high-R/low-S signatures pass strict verification. Two deliberately typed domain distinctions are recorded in the fixture: tiny returns false for zero scalars while this API returns `InvalidSignature`; tiny also accepts parity-consistent `06`/`07` hybrid keys, while this native API requires the explicit canonical SEC1 domain above. A source adapter that accepts hybrid keys must separately check their parity and normalize them before calling this primitive; it must label a stricter application policy separately.

## Mathematics and implementation bounds

The domain parameters are secp256k1's field `p = 2^256 - 2^32 - 977`, curve `y² = x³ + 7`, generator G and prime order n with cofactor one, as specified in [Standards for Efficient Cryptography 2 (SEC 2), version 2.0, §2.4.1](https://www.secg.org/sec2-v2.pdf). Canonical finite on-curve points therefore belong to the required prime-order subgroup; no encoded infinity is admitted.

The verification relation follows [Standards for Efficient Cryptography 1 (SEC 1), version 2.0, §4.1.4](https://www.secg.org/sec1-v2.pdf): compute `u1 = e/s` and `u2 = r/s` modulo n, form `R = u1 G + u2 Q`, reject infinity, and compare `R.x mod n` to r. Since the digest is exactly 256 bits and n has 256 bits, no additional hash truncation is needed; scalar reduction handles digest values at or above n. The `x mod n` step is tested with a constructed valid point whose x exceeds n, rather than relying on an astronomically unlikely random example.

Both field and scalar values use four little-endian `u64` limbs. Montgomery multiplication uses a nine-word product/reduction buffer and `u128` multiply/accumulate operations. An inner accumulation is bounded by `(2^64−1)^2 + 2(2^64−1) = 2^128−1`; each carry is explicitly retained. Canonical inputs below a modulus give a reduced result below twice that modulus, requiring at most one final subtraction. Addition handles the 257th carry explicitly; subtraction handles borrow explicitly. Montgomery constants are independently exercised by the BigInt corpus for both moduli.

The curve calculation uses Jacobian coordinates with explicit infinity/equal/opposite-point branches and a joint 256-round double-and-add. Its addition and doubling formulas correspond to the [Explicit-Formulas Database's `add-2007-bl` and `dbl-2009-l`](https://www.hyperelliptic.org/EFD/g1p/auto-shortw-jacobian-0.html). Only the final x-coordinate needs affine conversion. Inversion and compressed-key square roots use fixed bounded exponentiation. All internal data is fixed-size stack data; malformed oversized public-key slices fail their length check before copying or arithmetic. No attacker-selected loop count, allocation or exponent size enters production arithmetic.

This is a **variable-time public-input verifier**. It makes no constant-time, secret-handling, signing, formally verified or independently audited cryptographic-library claim. The stricter low-S policy is separate from mathematical ECDSA validity; [libsecp256k1's public API](https://github.com/bitcoin-core/secp256k1/blob/master/include/secp256k1.h) likewise documents its low-S verification policy and normalization distinction.

## Independent evidence

`fixtures/secp256k1_vectors.tsv` contains 1,497 records:

- **1,090 M records:** independently calculated BigInt modular add/subtract/multiply/inverse results for p and n, including all pairs of 17 zero/one/modulus/half-modulus/word-boundary/alternating-bit values and 256 deterministic SHA256-seeded pairs per modulus.
- **143 P records:** compressed/uncompressed canonicalization, OpenSSL ECDH cross-checks, canonical coordinate limits, bad prefixes/lengths, off-curve/nonresidue cases and explicit hybrid-domain observations.
- **252 V records:** genuine tiny-secp256k1 deterministic signatures, high-S equivalents, compressed/uncompressed keys, message/R/S/key mutations, digest-zero/n/max cases, zero/order/max scalar bounds, infinity sums and the rare high-x witness.
- **12 O records:** actual OpenSSL-produced compact ECDSA signatures over SHA256 messages, independently checked by Node/OpenSSL, tiny-secp256k1 and the affine BigInt verifier. Their random signatures are captured once and retained across fixture regeneration; their generation inputs and signatures are rechecked.

The Rust private arithmetic tests consume every M record, and additionally check group-order, infinity, equal/opposite-point and projectively equivalent-point cases. Integration tests consume every P/V/O record under both verification policies, check canonicalization idempotence, explicit high-R/low-S behavior, typed zero-scalar distinctions and oversized-key rejection.

Run the dependency-free independent oracle with:

```text
node scripts/secp256k1_vectors.cjs --check
```

This recomputes the full arithmetic corpus, parses/checks every point, verifies ECDSA with separate affine BigInt/extended-Euclid arithmetic, and rechecks OpenSSL signatures. It does not call the native Rust implementation. To additionally recheck genuine tiny-secp256k1, supply the pinned JavaScript checkout/package owner:

```text
node scripts/secp256k1_vectors.cjs --check --tiny-root PATH
```

`--write` requires that genuine package, preserves previously captured valid OpenSSL signatures, and cross-checks generated public points with OpenSSL ECDH. The tool pins package version 2.2.4 and exact SHA256s of `lib/cjs/index.cjs` (`3864bb04fb2a7a663038301f312010f462142c2555763600ae8eb3a7ecda8b01`), `lib/cjs/validate.cjs` (`f0638d147bac3b60764b2b68f79d72cdccad52523fcd0a98599b32e0873f58e2`) and `lib/secp256k1.wasm` (`5cbbcb8adc7acc3b78a3fd31c76d573302705ff5fd714d03f5a2602591197cb5`). The captured generator used Node 24.19.0/OpenSSL 3.5.7; the check reports runtime versions without treating a patch-version difference as a cryptographic acceptance rule. Both independent and genuine-library checks passed locally. Rust compilation, linting and runtime validation are run centrally by the parent agent.

This prerequisite does not complete Bitcoin script verification, authenticated parent-UTXO acquisition, DLC economic admission, a DLC execution proof or privileged actor authorization.
