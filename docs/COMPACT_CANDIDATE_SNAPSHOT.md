# Coherent, conditional compact candidate snapshots

`compact_candidate_snapshot` is a read-only reader for the existing native candidate writer's `TLCAND-MANIFEST-1` and `TLCAND-SEGMENT-1` formats. It repeats actual Rust raw-block discovery and compares the complete ordered canonical carrier set. It does not mutate writer files, implement storage maintenance, call economic execution, or provide chain/actor authority.

## Explicit context and publication

Open a snapshot with `CandidateSnapshotExpectation`: the named `NativeV1BitcoinCanonicalAllOutputs` profile, network, caller-retained anchor height/display hash, generation and raw SHA256d of exact CURRENT bytes. The expectation must come from the consumer's independently retained context; deriving it from the same untrusted publication does not authenticate that publication or its freshness.

The profile uses Bitcoin header/TX syntax and every canonical one-push output whose payload begins `tl` or `TLR`. This includes carriers whose payload syntax is invalid. It differs from literal txIndex's first-nulldata selection and does not claim that proposed TLR is activated TradeLayer consensus. Hash bytes in the anchor/block identities are in display order. Manifest and segment SHA256d bytes use the writer's raw digest order.

The reader parses one immutable CURRENT generation, validates its exact context and contiguous parent linkage, and requires generated segment basenames to match their height and content digest. Each block result carries the same snapshot token. CURRENT is checked before and after block discovery; a changed generation is an explicit `StaleSnapshot`, including A-B-A observations that return to the original block hash with a later GEN. Consumers must not combine results with different tokens.

This follows the existing writer's monotonic generation, immutable segment and atomic CURRENT contract. It does not independently prove Bitcoin's active best chain or detect a malicious producer restoring an old publication between observations. A missing block is `MissingHeight`, never an empty successful result.

## Completeness is rederived from supplied raw blocks

`read_block(height, raw_block)` requires full raw block bytes even for a segment claiming zero carriers. It runs existing `UnverifiedRawBlockView::parse` and `inspect_serial`, checking header identity, claimed parent, transaction count and the full TXID Merkle. It enumerates every canonical carrier in original transaction/output order.

Each stored C row must then match that exact expected position, TXID, vout, format, independently derived type/status, and full raw transaction. Missing, duplicate, extra or reordered rows fail even if END counts, segment digests, filenames and manifest pins are repaired. The final number of rows must equal the independently enumerated set. No completeness verdict is supplied by the caller or accepted from a sentinel.

The source block is retained once in the immutable result, with transaction ranges and output metadata. Raw transaction access borrows that buffer; multiple carriers do not duplicate entire transaction bodies. Result fields are private and exposed through immutable accessors.

The prior PR144 omission probe showed that a structurally valid four-TX/two-carrier block could be stored with no C rows and a matching END count. This reader's corresponding adversarial test repairs the native segment/manifest metadata around that same omission and requires `CandidateSetMismatch`.

## Bounds and filesystem assumptions

The reader bounds CURRENT and each segment to 64MiB, manifest entries to 200,000, block bytes to 4,000,000, candidate rows to 20,000 and individual transaction bytes to 1,000,000. Transaction count also obeys the existing Rust scanner's 200,000 limit. These are explicit bounded-reader limits, not a claim of indefinite node storage. The writer's separate manifest growth/publication-cap issue remains outside this module.

Reads use a bounded `Read::take` after file-handle size checks, including growth after stat. Field splitting allocates only the expected fixed number of fields. Paths are canonicalized under the selected store, segment names must exactly match `seg-<10-digit height>-<lowercase digest>.dat`, and known symlink/nonregular inputs or redirected segment directories are rejected. The local store directory is an operator-controlled filesystem boundary; portable standard-library path checks do not promise race-free defense against a malicious local process replacing filesystem entries. The Unix symlink regression is compiled conditionally.

The module deliberately rereads and hashes CURRENT before and after each block read. This is a bounded correctness boundary, not an optimized archival query engine. No files are created, locked, renamed, deleted or repaired by the reader. A stale writer LOCK does not itself grant or revoke read authority; only the pinned published generation is read.

## Authority and remaining work

Successful structural completeness is conditional on these exact raw block bytes and the named carrier profile. The reader does not establish greatest-work/active-chain membership, PoW validity, BIP141 witness commitments, UTXO state, input-script signatures, sender/actor authorization, activation history or TradeLayer balance correctness. The result's three authority requests always return explicit errors.

Continuous Core snapshots, TLCR segmentation, compaction, replay/state admission, reorg rollback of financial execution, SQLite integration and general NeDB emulation remain separate work. This module has no Ledger or Core state dependency and introduces no second spendable authority.

## Verification and integration

The baseline manifest/segment constants in the tests were generated by executing the unchanged original PR145 `render_manifest` and `segment_bytes` functions at head `6e9ab5541e647085a6c0b78ec29139c26b85b2c5`, Git blob `0998c1177588fc5b6ede00a94cad7afc02b9ba1b`. Their C rows came from the actual Rust scanner on the repository's existing 370-byte/four-TX/two-carrier block. Work evidence pins each extracted serializer function. This verifies format interoperability; the Linux writer's fsync/publication protocol was not exercised on Windows.

Rust1.90 MSVC standalone test-linking used an immutable copy of Root's existing V9 rlib, SHA256 `90e478a58b0bbe79a35dbaa13c8a3349f357d2ff3ef27933abfe86c7e986a64a`. Only test import/path resolution was projected to include the new reader directly. The module and assertions were unchanged. All 13 portable reader tests plus two existing SHA known-answer tests passed; the additional Unix symlink test is pending an appropriate host. Rustfmt and a standalone Clippy metadata check with warnings denied passed. No whole-library build was run in this work lane.

The module is exported from `src/lib.rs`. Root built the candidate library and passed `cargo test --frozen --test compact_candidate_snapshot`: 15 tests, including 13 reader tests and two fixture SHA known answers. Complete-suite promotion is recorded separately. The three proposed files are `src/compact_candidate_snapshot.rs`, `tests/compact_candidate_snapshot.rs` and this document; PR145 writer files are unchanged.
