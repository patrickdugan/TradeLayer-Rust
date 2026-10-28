Root integration note: the focused checks recorded in [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md) have passed. The staged handoff below is preserved for provenance; its work-only/uncompiled statements describe the earlier handoff, not the current Root integration. Full release promotion remains pending.

# Explicit Litecoin transparent ingestion: initial restricted profile

This is an opt-in, unverified reader for transparent transactions, HogEx and an absent or empty-body MWEB block. It is an implemented initial subset; the Litecoin ingestion roadmap remains **partial** until genuine nonempty MWEB cases support a further profile. Bitcoin/default transaction parsing, block parsing and node admission are unchanged.

The proposed API is `read_only::litecoin_transparent`. Callers must explicitly supply `LitecoinTransparentProfile::Core021EmptyMwebV1`. Standalone transaction parsing requires exact length. Block parsing rederives SHA256d header identifiers, transparent TXIDs and witness IDs, rejects duplicate transparent IDs, checks the header's transparent Merkle root and consumes the extension to its exact end. Existing canonical CompactSize, OP_RETURN push extraction and TradeLayer carrier classification helpers are reused. Source bytes are borrowed and retained unchanged.

## Official source and examples

The source pin is Litecoin Core **beae01d62292a0aab363b7a4d3f606708cea7260**, which is also the official **v0.21.4** tag. Twenty-four source files were fetched through GitHub and their exact Git blob identities verified; their work-stage inventory is `official-provenance.json`.

The official [MWEB mining example](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/doc/mweb/mining-changes.md) contains a complete 944-byte submitted-block example. Its three transactions occupy `81..291`, `291..676` and `676..773`; the MWEB optional/header/empty-body extension occupies `773..944`. Its declared example height is 2,273,263. This is genuine official example data, not evidence that this block is an active mainnet block.

`scripts/extract_litecoin_official_examples.py --check` verifies the exact pinned source Git blob and independently derives boundaries, SHA256d IDs and the transparent Merkle root with Python's standard hashing library. It checks the two declared transaction IDs, witness IDs and transaction weights against the official example, then compares the generated fixture bytes. It does not generate expected results from the new Rust parser. The exact official document is retained in `fixtures/litecoin_core_mining_changes_source.txt`.

Three pre-MWEB genesis examples (mainnet, testnet and regtest) are reused from the independently checked official chainparams evidence. Their SHA256d identifiers and transparent Merkle roots are checked. The parser accepts common serialization under an explicit caller-selected profile; parsing alone cannot determine a trusted network or authenticate a chain.

## Serialization rules actually supported

[Core transaction serialization](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/primitives/transaction.h) uses optional flag `0x01` for witness and `0x08` for MWEB. This profile accepts only `0x01`, `0x08` or `0x09` after the zero marker. Unknown flags are rejected. A set MWEB flag followed by an explicitly null optional transaction identifies HogEx, which requires transparent outputs. HogEx must be the last of at least two transactions in the supported block profile.

[Core transaction hashing](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/primitives/transaction.cpp) excludes both witness and MWEB for transparent TXIDs. Witness IDs exclude MWEB and fall back to the TXID when there are no witness stacks. Thus hashing the complete 97-byte HogEx envelope would produce the wrong identifier; its transparent serialization is 94 bytes and its official witness ID equals its TXID. Flag `0x09` is supported for canonical witness plus null-MWEB HogEx framing, with synthetic framing tests clearly labeled as such.

[Core block serialization](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/primitives/block.h) adds the optional MWEB block after the transaction vector only when the final transaction is HogEx. A null optional extension (`0`) is structurally representable. A present extension (`1`) requires the complete [MWEB header](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/src/libmw/include/mw/models/block/Header.h): nonnegative signed-32 height VARINT, three 32-byte roots, two 32-byte offsets, and two unsigned-64 MMR-size VARINTs. These VARINTs use Core's MSB-base128 continuation-plus-one encoding, not CompactSize or LEB128. Numeric overflow is rejected. The following input/output/kernel CompactSize counts must each be zero in this named profile.

The profile deliberately rejects optional-presence values other than `0` and `1`, although Core's generic optional-pointer decoder treats other non-`1` values as null. This is an explicit canonical subset, not a claim to accept every permissive Core decoder encoding.

Resource limits are 1,000,000 bytes per standalone transaction, the existing bounded input/output/stack counts, the existing maximum transaction count, a 4,000,256-byte raw block cap and a 4,000,000 transparent weight cap excluding MWEB serialization. The extra raw bound permits only bounded extension overhead; it does not enable arbitrary tails. Parsing rejects every truncation of the complete official example and all trailing bytes.

## Explicitly unsupported paths and remaining codec fields

A non-null MWEB transaction returns `UnsupportedNonNullMwebTransaction`; a pure MWEB-only object returns `UnsupportedMwebOnlyTransaction`. Pure MWEB-only identifiers are kernel-derived in Core and must not be represented as SHA256d transparent TXIDs. Any nonzero extension-body count returns `UnsupportedNonemptyMwebBody`. No success path skips an unknown flag or an unparsed byte sequence.

The following exact pinned C++ fields are mapped for the next profile, but are **not yet admitted**:

| Object | Remaining serialized fields/cases |
|---|---|
| MWEB transaction | 32-byte kernel offset, 32-byte stealth offset, body; at least one kernel required by Core. Transparent-plus-MWEB and pure-MWEB envelopes need separate cases. |
| Body | Canonical CompactSize vectors of actual inputs, outputs and kernels; multi-item and allocation/aggregate bounds. |
| Input | Feature byte, output ID 32, commitment 33, output public key 33; optional stealth input public key 33 (bit 1), optional CompactSize extra bytes (bit 2), signature 64. |
| Output | Commitment 33, sender key 33, receiver key 33, message, fixed 675-byte range proof, signature 64. |
| Output message | Feature byte; bit 1 adds key-exchange key 33, view tag 1, masked value 8 and masked nonce 16; bit 2 adds CompactSize extra bytes. |
| Kernel | Feature byte; fee/pegin nonnegative signed-64 VARINT (bits 1/2), CompactSize pegout vector (bit 4), nonnegative signed-32 height VARINT (bit 8), stealth-excess public key 33 (bit 16), CompactSize extra bytes (bit 32), excess 33 and signature 64. |
| Pegout | Nonnegative signed-64 amount VARINT plus CompactSize script bytes. |

Known feature masks, zero/multiple vector cases, every optional combination, count/length overflow, malformed/truncated proofs and keys, and exact extension-end framing all need genuine reference evidence before promotion. No future profile should generically ignore unfamiliar feature bits.

The [official Python helper](https://github.com/litecoin-project/litecoin/blob/beae01d62292a0aab363b7a4d3f606708cea7260/test/functional/test_framework/messages.py) is insufficient as a full nonempty oracle: its pegout decoder has a TODO that consumes no vector, and its kernel stealth-excess decoding uses a 32-byte `Hash` where the C++ decoder reads a 33-byte `PublicKey`. Its BLAKE3 and Litecoin-Scrypt runtime dependencies are also unavailable here. Neither placeholders nor substituted hash answers were used.

Pinned libmw input/output/kernel/block tests generate randomized objects and round-trip them at runtime, without checked-in static nonempty byte vectors. The pinned `tx_valid.json` contains no MWEB optional envelopes. A real nonempty block/transaction corpus and genuine reference execution are still missing.

## Isolated official-binary oracle option

Official v0.21.4 is the exact pinned source commit. Its portable Windows archive is [litecoin-0.21.4-win64.zip](https://github.com/litecoin-project/litecoin/releases/download/v0.21.4/litecoin-0.21.4-win64.zip), 34,030,019 bytes, with release-published SHA256 `70a45828ef3042de9b8d07d4610b2714d2b0047689b2b0c65e035207284dfdc2`. All 13 audited serializer blobs match that tag. The work evidence `reference-binary-assessment.json` records exact provenance. No binary was downloaded, installed or executed: no installed/archive copy was found, and the available shell cannot resolve the official download hosts. Source/release metadata access through the GitHub connector works; binary-asset download does not.

Once the exact portable archive is available, verify its published digest and preserve the binary version/digests. Extract only inside a fresh work directory. Run only regtest with a separate explicit datadir/config, outbound networking and listening disabled, loopback-only RPC, and a hidden process. Never use an existing wallet/datadir or system installation. Confirm zero peers and disabled networking before generating fixtures, and stop the process afterward.

The pinned `test/functional/test_framework/ltc_util.py::setup_mweb_chain` supplies a concrete reference sequence: generate 431 blocks, send one regtest coin to a genuine newly generated MWEB address, then generate the first MWEB block at height 432. Capture Core's raw block, raw transparent/HogEx transactions, verbose IDs and MWEB fields. Extend using `mweb_basic.py` for MWEB transfers and pegout, then capture an empty-body follow-up and boundary cases. Core's decode/serialize results, not this Rust reader, must supply the nonempty oracle. The official libmw tests remain the reference for less common extra-data/feature combinations.

## Verification and remaining integration

The standalone Rust 1.90/edition-2021 suite currently passes 13 tests: eight new adapter tests plus five existing parser/SHA tests. It links unchanged providers from an immutable copied rlib, SHA256 `90e478a58b0bbe79a35dbaa13c8a3349f357d2ff3ef27933abfe86c7e986a64a`. Tests cover official pre/post examples, IDs/weights/Merkle, exact boundaries, all 944 truncations, mutations, count/size/VARINT limits, duplicate IDs, HogEx ordering, canonical flags, refused unimplemented paths, existing carrier classification and unchanged default Bitcoin rejection. Synthetic framing cases claim no mined-chain or cryptographic validity.

The work-only `prepare_read_only` transform appends a child module to the current `read_only.rs`; the entire existing source text remains intact. Root owns mounting, full-build/CI validation and committing. No whole-Core build or checkout modification was performed by this lane. This adapter does not verify Scrypt work, active-chain status, MWEB commitments/proofs/signatures, UTXOs/scripts, transaction validity, actor authorization or TradeLayer state. Its trust gate always returns an unverified error, and it is not connected to node admission.
