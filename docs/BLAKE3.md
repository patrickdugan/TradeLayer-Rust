Root integration note: the focused checks recorded in [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md) have passed. The staged handoff below is preserved for provenance; its work-only/uncompiled statements describe the earlier handoff, not the current Root integration. Full release promotion remains pending.

# Native unkeyed BLAKE3-256

The module exposes one function:

```rust
pub fn blake3_256(input: &[u8]) -> [u8; 32]
```

It hashes exactly the borrowed input, returning the ordinary32 natural digest bytes. There is no prefix, byte reversal, length framing, personalization, keyed mode, key derivation or extended-output API. The implementation is first-party scalar safe Rust1.90, with no crates, unsafe code, input copying or heap allocation. It does not parse the input, authorize an actor or establish chain membership.

## Specification and independent reference

The [official specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/ac784c9f22a48327782f042ec2f4d8126b3b1744/blake3.tex) is pinned to `BLAKE3-team/BLAKE3-specs@ac784c9f22a48327782f042ec2f4d8126b3b1744`, Git blob `7c270d3773bbe9cbfe6b7b9fee08a4f041a236bb`. The independent reference and published vectors use official BLAKE3 v1.8.2, commit `df610ddc3b93841ffc59a87e3da659a15910eb46`.

The [published test vectors](https://github.com/BLAKE3-team/BLAKE3/blob/df610ddc3b93841ffc59a87e3da659a15910eb46/test_vectors/test_vectors.json), Git blob `f6da91792c6cdf5c6a0f6dad01803045bb204a68`, specify repeating input bytes0 through250 and extended outputs whose first32 bytes are the default digest. All35 unkeyed cases are included literally, covering empty input, partial/full64-byte blocks,1023/1024/1025-byte chunk boundaries, uneven trees, exact powers of two and100 chunks. Keyed/KDF entries are not treated as unkeyed answers.

For independent execution, the unchanged official [C implementation](https://github.com/BLAKE3-team/BLAKE3/blob/df610ddc3b93841ffc59a87e3da659a15910eb46/c/blake3.c), portable compression implementation, dispatcher and headers were fetched by exact Git blob. An isolated C driver uses ordinary `blake3_hasher_init`, `update` and `finalize`. It was compiled with the installed MSVC compiler, disabling every SIMD backend. It reproduced all35 official cases before producing30 additional boundary/uneven-tree checks up to1048577 bytes and verifying the three genuine Litecoin kernel vectors. The source hashes, exact compiler invocation and executable hash are recorded under the work stage's `reference/provenance.json` and `reference_results.json`.

No official/vendored Rust package was copied into the implementation or added as a dependency. Official C/specification snapshots stay outside the three-file payload and serve only as pinned verification evidence.

## Chunk, tree and root behavior

Compression uses the standard IV, seven BLAKE2s-style mixing rounds, the BLAKE3 word permutation and little-endian message/output words. The state directly contains the64-bit counter halves, actual block length and flags. It does not use BLAKE2's counter-IV XOR initialization.

Input is partitioned into1024-byte chunks and64-byte blocks. Every chunk's first/final block receives `CHUNK_START`/`CHUNK_END`; partial blocks are zero-padded but retain their actual byte length. Empty input has one zero-length block with both chunk flags. Exact full chunks do not acquire an extra empty block or chunk. Chunk counters are absolute indexes in the complete input.

For a non-leaf node, the left subtree is the largest complete power-of-two chunk subtree smaller than the entire node; this gives the required complete, larger-or-equal left subtree, including uneven trees. Parent input concatenates the two child chaining values, with IV initialization, counter zero, block length64 and `PARENT`. `ROOT` is applied only to the final compression at the actual root: a lone chunk's final block or the final parent. Since this API returns only output bytes0–31, output block counter zero is sufficient. A non-root chaining value is not returned as the digest.

The implementation borrows slices recursively and uses fixed small local arrays. Stack depth is logarithmic in input length and bounded by the platform's input-length bit width; it does not allocate a vector of leaves or recurse once per input byte. Left-split multiplication remains within the borrowed slice, and all compression additions use explicit32-bit wrapping arithmetic. No mutable globals or reuse state exist. SIMD, streaming, parallel scheduling and XOF modes are outside this small API.

## Litecoin MWEB preimages and byte order

Pinned Litecoin Core `beae01d62292a0aab363b7a4d3f606708cea7260` [Hasher.h](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/libmw/include/mw/crypto/Hasher.h), Git blob `7feedda29b0c1e584b9a830bccff50d4e2011048`, uses ordinary unkeyed BLAKE3 initialization/update/finalization without a tag prefix. The parser worker supplies the canonical [Kernel serialization](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/libmw/include/mw/models/tx/Kernel.h), Git blob `d114e624a421161bee94868c6c4efee23424a2d7`.

All three supplied genuine Core-produced kernel preimages—pegin138 bytes, pure MWEB transfer133 bytes and pegout164 bytes—agree between Core's natural hash, the independently executed official C reference and this Rust implementation. Tests contain the exact preimages and natural/display digests. Empty conditional vectors may be omitted by the canonical source serializer despite set feature bits; identifying that canonical preimage belongs to the parser, not this hashing primitive. Hashing a consumed wire span blindly is not claimed equivalent.

The API returns natural digest order. Litecoin `uint256` display identifiers reverse those32 bytes once; the test checks that boundary explicitly. For the supplied pure-MWEB examples, Core TXIDs equal the reversed canonical kernel hash; the pegin envelope has its distinct transparent SHA256d TXID. This module does not select a TXID scheme or replace Litecoin's SHA256d block identifiers or separate Scrypt proof-of-work hashing.

## Validation and integration boundaries

Four isolated Rust tests compare all35 official cases, all30 independently executed C cases including unaligned borrowed slices, all three genuine kernel preimages, and independent parallel/repeated calls. The standalone crate contains only this module and the tests and forbids unsafe code. It has no dependencies and its own target directory. Rust1.90 debug/release tests, Rustfmt and Clippy all-targets with warnings denied are recorded in the work-stage check report.

Root later owns module registration (`pub mod blake3;`), whole-Core integration checks and commit. The proposed payload contains only `src/blake3.rs`, `tests/blake3.rs` and this document. No existing Root file, parser/profile admission, follower or default behavior was changed here. Matching a digest establishes neither authenticated context nor canonical source serialization, MWEB commitments/validity, chainwork, scripts/UTXOs or actor authority. Those independent checks remain explicit consumer responsibilities.
