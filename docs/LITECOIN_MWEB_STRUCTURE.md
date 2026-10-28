Root integration note: the focused checks recorded in [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md) have passed. The staged handoff below is preserved for provenance; its work-only/uncompiled statements describe the earlier handoff, not the current Root integration. Full release promotion remains pending.

# Litecoin MWEB wire structure and identity

`Core021MwebStructureV2` extends the explicit unverified Litecoin reader to nonempty extension blocks, transparent-plus-MWEB envelopes and pure-MWEB envelopes. It parses all known pinned C++ feature fields, computes the correct kind of identifier, and reproduces the original serializer's observable defects. The older `Core021EmptyMwebV1` profile retains its narrower refusals. Bitcoin/default parsing and node admission remain unchanged.

The reference is official Litecoin Core **v0.21.4**, commit **beae01d62292a0aab363b7a4d3f606708cea7260**. This is wire extraction and fingerprinting; it does not establish script/UTXO validity, MWEB proof/signature validity, active-chain membership, actor authorization or TradeLayer state. The full-consensus gate still always returns an error.

## API

Use `read_only::litecoin_transparent::UnverifiedLitecoinTransparentBlock::parse` with the explicit V2 profile for a full borrowed block. It checks transparent SHA256d IDs, duplicate IDs, transparent Merkle root, transparent weight and exact extension end. The extension's complete header and input/output/kernel vectors remain available as unverified typed fields and borrowed byte ranges.

Use `UnverifiedLitecoinTransactionEnvelope::parse` for standalone hybrid or pure-MWEB bytes. Its identity is an enum:

| Variant | Actual identifier rule |
|---|---|
| `TransparentSha256d` | TXID excludes witness and MWEB; witness ID excludes MWEB and equals TXID without witness. |
| `MwebFirstKernelBlake3` | Pure-MWEB Core TXID is the first canonical kernel's unkeyed BLAKE3-256 digest interpreted as uint256. It is not a transparent SHA256d TXID. |

`transparent()` returns no transparent observation for a pure-MWEB object. The transparent block/transaction entry points continue to refuse pure-MWEB objects rather than mislabel them. Natural kernel digest bytes are returned as `kernel_id`; `read_only::id_hex` reverses the digest for Core's display TXID. Kernel JSON's natural `kernel_id` and display TXID therefore appear reversed.

Output IDs use the pinned C++ composite hash: commitment, sender key, receiver key, BLAKE3(message), BLAKE3(range proof), then signature, all hashed with unkeyed BLAKE3. MWEB header fingerprints hash its actual serialized header. These are fingerprints, not proof or signature checks. Transaction field ranges are relative to that transaction's `raw_bytes()`; extension-body ranges are relative to the block's `raw_bytes()`.

## Literal framing and source behavior

The implementation follows [transaction serialization/hashing](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/primitives/transaction.cpp), [MWEB body](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/libmw/include/mw/models/tx/TxBody.h), [kernel codec](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/libmw/include/mw/models/tx/Kernel.h) and [Hasher](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/libmw/include/mw/crypto/Hasher.h). Native unkeyed BLAKE3 is supplied by the independently verified `blake3` module, source SHA256 `1f1244c4dce1215fd5fce56fa094ef09f425ca204be2372189d12838a9462119`.

Known feature masks are input `0x03`, output-message `0x03` and kernel `0x3f`. Fields include optional input stealth keys and extra bytes; output key exchange, view tag, masked value/nonce and extra bytes; fixed 675-byte proofs and 64-byte signatures; kernel fee, pegin, pegout vectors, height lock, 33-byte stealth excess and extra bytes. Core's nonnegative signed-64/signed-32 VARINT bounds are enforced. Empty pegout scripts and transactions without kernels fail as in the original reader. Unknown feature bits are an explicit unsupported capability, even when Core's permissive decoder accepts them; no unknown field is generically skipped.

Optional-presence bytes are restricted to canonical `0`/`1`, as in V1. Counts use canonical CompactSize and remain bounded by existing parser limits. V2 permits at most 32 MiB total block bytes; standalone envelopes retain the 1,000,000-byte bound. Per-vector and per-variable-field limits remain explicit. These are local resource limits, not claims about every consensus-valid maximum-size Litecoin block.

`canonical_bytes()` means **the original Core Serialize result**, not a repaired valid encoding. Core's kernel serializer omits an empty pegout vector and empty extra-data vector even when their feature bits remain set. Three executed cases show that Core accepts the original input, emits different bytes, then fails to decode its own emitted bytes. Rust reproduces those exact bytes and computes the source-compatible kernel ID from them. It does not silently insert the missing vectors or hash the wrong raw consumed preimage.

Raw body order is retained. Core's transaction JSON exposes output IDs through a `std::set`, which can reorder after message mutation. Tests compare that set's complete IDs separately while checking the original ordered wire representation exactly.

## Genuine reference evidence

The official portable ZIP has published SHA256 `70a45828ef3042de9b8d07d4610b2714d2b0047689b2b0c65e035207284dfdc2`. Every ZIP path was checked beneath the work-only extraction directory; only reference executables were extracted. Reference executable SHA256 values are:

| Executable | SHA256 |
|---|---|
| `litecoind.exe` | `151ad090a79223c01a4c317347d5dda7be6be6f7b167e65cdfbe37dbe4d1648c` |
| `litecoin-tx.exe` | `050f54e1aadf3724d3188bd0901534e1e753bec295cba1aa00a2771b190ef469` |

The daemon used a new explicit regtest datadir/config, hidden owned processes, disabled networking/listening/discovery and loopback-only RPC. Actual RPC responses verified `chain=regtest`, `networkactive=false` and zero connections before wallet/mining actions. No system installation, user wallet or external broadcast was used. All owned helpers are stopped, with stop acknowledgement and process exit verified.

The genuine recorded chain has seven complete blocks: pre-MWEB 431, first pegin 432, empty follow-up 433, transfer 443, pegout 444, multiple-kernel 445 and witness-hybrid 453. Four complete envelopes cover a legacy-input pegin, pure-MWEB transfer, pure-MWEB pegout and genuine witness-plus-MWEB flag `0x09` pegin. Tests compare every exposed Core header/input/output/kernel field, proofs as bytes, transparent IDs/witness IDs, Merkle roots, weights, MWEB output/kernel/header fingerprints and exact byte roundtrips.

The separate unchanged offline `litecoin-tx.exe` decoder/serializer ran **29 explicit derived-wire cases**: 22 supported known-field successes, four genuine source decoder errors and three accepted-by-Core future-feature cases that Rust explicitly refuses. Cases exercise every known optional feature branch, zero and boundary values, multiple pegouts, extra data across a BLAKE3 chunk boundary, empty-vector omissions, and two-kernel envelopes in both orders. Pure-MWEB IDs follow the actual first kernel in both orders. Derived inputs characterize the codec; their signatures/proofs are not claimed valid.

Goldens come from the official executables, not the new Rust parser. `scripts/project_litecoin_core_goldens.py --check` deterministically projects recorded Core JSON and raw bytes. With the exact official offline executable available, `scripts/litecoin_codec_reference.py --tool <litecoin-tx.exe> --check` reruns all 29 decoder/serializer observations. Full provenance and fixture hashes are retained in the package.

## Validation and integration gates

Actual standalone Rust 1.90/edition-2021 debug and optimized checks pass, including unchanged V1/default regression tests, genuine Core parity, every truncation of all eleven records, trailing-data rejection, bounded sizes/counts and explicit future-feature refusals. An opaque proof-byte mutation remains structurally parseable, changes its fingerprint and still fails the full-consensus trust gate; it is never promoted to MWEB validity.

The unchanged existing parser/wire/Merkle providers were linked from immutable rlib SHA256 `90e478a58b0bbe79a35dbaa13c8a3349f357d2ff3ef27933abfe86c7e986a64a`. No whole-Core build or checkout modification was performed in this lane. Root owns mounting the explicit reader and the separate BLAKE3 module, running full integration/release checks and committing. The pure `prepare_read_only` transform appends/reuses the child module without changing default admission.

This closes the missing known-feature nonempty wire-reader/identity evidence. Production Litecoin admission, authenticated history, MWEB consensus/UTXO/script validation, state authorization and throughput promotion remain separate gates. Hash agreement does not supply any of those authorities.
