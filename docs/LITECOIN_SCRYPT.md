# Litecoin header ID and Scrypt PoW digest

`litecoin_scrypt` is a dependency-free, safe Rust1.90 fixed-parameter header hashing primitive. It exposes two deliberately separate calculations:

```rust
use tradelayer_core::litecoin_scrypt::{litecoin_header_id, litecoin_scrypt_pow_hash, LitecoinScrypt};
let header = [0u8; 80]; // Caller supplies the actual serialized header bytes.
let identifier_raw = litecoin_header_id(&header); // SHA256d, used by Litecoin RPC IDs.
let pow_raw = litecoin_scrypt_pow_hash(&header); // Separate Scrypt work digest.
let mut workspace = LitecoinScrypt::new();
assert_eq!(workspace.hash(&header), pow_raw); // Reuse for subsequent headers.
```

Neither calculation validates a block or chooses an active chain. This proposal adds no follower admission, body parser, target/difficulty verification, MWEB validation, chainwork check, script/UTXO validation or TradeLayer authentication. Existing network/body/consensus/trusted-context refusals remain in place.

## Source pin and parameters

The reference is official Litecoin Core v0.21.4, commit `beae01d62292a0aab363b7a4d3f606708cea7260`:

- [CBlockHeader hashing](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/primitives/block.cpp#L14-L24): `GetHash()` is SHA256d serialization hashing; `GetPoWHash()` invokes Scrypt.
- [Generic Scrypt implementation](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/crypto/scrypt.cpp): password=salt=the80 header bytes, PBKDF2-HMAC-SHA256 with one iteration, ROMix N1024/r1, one parallel block, final32-byte PBKDF2 output. The primitive reproduces these fixed parameters using safe Rust, without importing C/SIMD code.
- [Five official known-answer tests](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/test/scrypt_tests.cpp): copied literal input/output vectors in the Rust integration test. Their downloaded Git blob is `d037c88d350d24360be12befdf71effc5d4d5885`; the generic Scrypt source blob is `de9f6e4c0e4f87986894eac2724f15647dc273b0`.
- [Genesis construction and assertions](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/chainparams.cpp): main, test and regtest header inputs and SHA256d IDs are pinned. The prior ID-vs-PoW audit independently reconstructed their coinbase/Merkle/header bytes; its immutable `vectors.json` SHA256 is `564e5d3acf716d1e5a099f757aa1c70dbb9f7d8d8a1e324c62ef1362a59eac45`.

The module reuses `crate::hash256::{sha256, double_sha256}` without changing them. Those existing functions remain subject to their repository's research/audit limitations. Known-answer agreement here does not promote either implementation into a fully audited consensus verifier.

## Bytes, integer words and display order

The API accepts `&[u8;80]`, the exact wire serialization of version, previous block ID, Merkle root, time, bits and nonce. It neither reverses the complete header nor reserializes native integers. PBKDF2 block indexes are big-endian; ROMix/Salsa words are little-endian. With N1024, Integerify's low ten bits are exactly the low ten bits of word16 from the final64-byte block. Salsa20/8 uses wrapping32-bit addition and four column/row double rounds.

Both exported hash functions return the raw32 digest bytes, matching bytes written to Core's `uint256` storage. Core's usual hexadecimal `GetHex()`/`ToString()` representation reverses those raw bytes. Reverse **once** when displaying or comparing with RPC hex; do not reverse bytes before a raw-digest comparison. Root's `BlockId` uses display-order bytes, while the raw ingest parser expects raw digest bytes, so callers must use the appropriate boundary conversion. If a later target comparison is implemented, it must interpret the PoW digest as Core's little-endian arithmetic integer and separately validate the network/context-specific target; string lexicography over raw hex is not that check.

| Network | SHA256d RPC identifier | Separate Scrypt PoW digest, Core display order |
|---|---|---|
| Main | `12a765e31ffd4059bada1e25190f6e98c99d9714d334efa41a195a7e7e04bfe2` | `0000050c34a64b415b6b15b37f2216634b5b1669cb9a2e38d76f7213b0671e00` |
| Test | `4966625a4b2851d9fdee139e56211a0d88575f59ed816ff5e6a63deb4e3e29a0` | `000006cc0225c4b4c387604dd670b1ff4b95af0f46f86bef805c0d085b60de64` |
| Regtest | `530827f38f93b43ed12af0b3ad25a288dc02ed74d6d7857862df51fc56c416f9` | `5adbca3495032eb0f2344f78ad238a3abb56d335b1680f5c289276e0d25a3479` |

## Bounded memory and work

One `LitecoinScrypt` workspace holds1024 rows of32 `u32` words: exactly131072 bytes of ROMix payload on the heap, plus allocator metadata. Working arrays and SHA state are small fixed stack allocations. A one-shot call allocates one workspace; callers hashing a sequence can reuse one instance. The first pass overwrites every table row before the second pass reads any selected row, so previous headers do not affect subsequent outputs. Separate workspaces have no mutable global state and can be used on independent threads.

The public API cannot change N/r/p, request an arbitrary derived-key length, pass arbitrary-length input or trigger input-sized allocation. Every hash performs1024 storing BlockMix calls and1024 lookup BlockMix calls, with two Salsa20/8 operations each. These bounds do not limit how many hashes a caller schedules or how many workspaces it creates. Standard Rust allocation failure behavior is retained; no custom allocator recovery is claimed. This is specifically Litecoin's public-header hash, not a general password-storage API.

## Verification and integration

Four new Rust tests cover all five official vectors, all three genesis identifier/PoW pairs,34 independently generated Python/OpenSSL `hashlib.scrypt` cases (32 deterministic SHA256-derived headers plus all-zero/all-ff headers), reverse-order workspace reuse and five concurrent independent workspaces. The tests compare exact raw/display byte order and both one-shot/reusable interfaces. The isolated harness also executes both unchanged SHA256 unit tests. Debug and release testing, Rustfmt1.90 and Clippy all-targets with warnings denied are recorded in the work-stage verification log and manifest.

The harness is a separate dependency-free crate containing only immutable copies of Root `hash256.rs` at commit `b02a0454e4da34c4c167b50f93dc579bda9e75ec`, the new module and these tests. It uses the installed `1.90.0-x86_64-pc-windows-msvc` toolchain and its own target directory. A harness-only dead-code annotation covers unused functions in the unchanged copied SHA utility; the new module has no lint suppression. No whole Core library build or Root edit was performed.

Root can later copy the three payload files and add `pub mod litecoin_scrypt;` through its reserved `src/lib.rs`, then run full integration checks and commit. The module-registration edit is intentionally outside this three-file proposal. No crates, unsafe code, follower/profile changes or default behavior changes are included. Full Litecoin raw-block support still requires MWEB/HogEx-aware parsing and network limits; PoW/chain acceptance still requires contextual target/difficulty and chainwork validation or explicitly trusted node context. Dogecoin AuxPoW remains a separate unimplemented format/validation path.

## Root integration check

The public module is registered in `src/lib.rs`. All four integration tests passed against the Root candidate library. Complete-suite promotion is separate; follower, body parsing, difficulty, active-chain, script and TradeLayer admission are unchanged.
