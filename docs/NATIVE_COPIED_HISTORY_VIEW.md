# Verified copied-generation metadata views

This is an additive metadata-reader prerequisite over Root `306d4186327a8d43d0fdf4c23c89563d3c0bb111`. It does not admit Mode4 histories to the existing cold service runner. Root owns the whole-Core gate and commit.

The three implementation includes expose strict readers for the existing storage escrow descriptor and source escrow scalar fields, plus `VerifiedOriginalMetadataViewV1`. A decoded descriptor is an independently retained expectation, not a synced-copy capability or a native writer permit. Actual open still checks the real record, manifest, copied log, complete historical KV and row receipts.

`VerifiedOriginalMetadataViewV1::open_original` reads actual original history through the existing locked index. `open_copy` opens actual independently pinned history in the copied generation. `original()` always retains the original history/physical facts. `copied_file()` and `copied_generation()` expose the distinct copy location/evidence. No copied snapshot publication is relabelled as the original publication. Public reads are `get` and complete streaming `visit_rows`; raw KV access stays crate-private for a later genuine historical inline preparer.

## Mount

Apply the four pure transforms in `prepare.py` to the current Root text, preserving later additions:

| Existing Root file | Transform |
| --- | --- |
| `src/compact_journal.rs` | `prepare_compact` |
| `src/source_compact_index_v2.rs` | `prepare_index` |
| `src/compact_generation_escrow_tests.rs` | `prepare_compact_tests` |
| `src/source_index_generation_escrow_tests.rs` | `prepare_index_tests` |

Copy the five `src` leaves listed in the manifest. They require the already committed independent escrow reader and history-pin scalar codec. No runtime, TxIndex, WAL, policy, archive, financial owner, catalog writer or pruner body is replaced. `preview/` is a hash-bound review result, not permission to overwrite newer Root source.

Both descriptor decoders check a 1 MiB raw bound before parsing, canonical field order/encoding, namespace, profile and aligned original/storage/row points. The binary descriptor also checks its independently retained hash, path syntax and generation/file bounds. File verification remains a separate step; hashes are not authentication. Existing writer encodings and live permit construction are unchanged.

## Evidence

Five new actual-file metadata tests cover strict wire parsing, unavailable/corrupt copies, an independently pinned point not listed in the original capture request, and 1,100 original rows after real compaction/pruning removes the original generation. The full isolated metadata/owner closure passes 128 tests, with one existing child helper ignored; warnings-denied Clippy passes. These are not whole-Core executions. The helper links the immutable actual provider pinned in `CHECKS.json`; all local helper inputs are retained for reproduction.

The retained first old-view test failure was a test setup error: `import_row` correctly refuses a conflicting migration ID. The corrected test uses the actual private `put` operation to produce a later metadata generation. No production reader was relaxed.

## Still open

Mode4 cold ACK reader/admission, semantic recovery-chain validation, historical inline database preparation from this actual old KV view, and both cold-prefix reconstruction and service-loop integration remain unimplemented here. The existing HISTORY gate remains strict. Retained Main callers also need their explicit caller/job/scope/operation ordinal and Awaited/Detached role bound before broader Mode4 selection. This package does not convert a checksum, expected pin or decoded filename into a live append permit, does not execute finance from recorded answers, and does not claim full cold service recovery.

## Actual Root gate

Root integrated the exact frozen package through its four include-only transforms. The two descriptor and three old-view tests pass inside the actual Core crate; the combined19 escrow controls also pass. All-target warnings-denied Clippy, Cargo fmt,272 included-file formatting and route inventory pass. Counts overlap. The1,100-row control performs actual generation compaction/pruning, then streams the original rows from the separately verified copy. No expected financial state is imported, and no financial/runtime/WAL body changed.

Verification: `fixtures/reference_fidelity/copied-history-view-core-integration.json`. The full release baseline is2,142 Rust/239 locked numeric checks at implementation53e6a24/tested306d418; these later five metadata controls await a future full-release promotion. Historical inline preparation, Mode4 cold admission/recovery-chain closure, full retained callers and process-loss finance remain open.
