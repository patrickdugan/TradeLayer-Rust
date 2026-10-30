# Root validation

Three exact old-body comparisons pass. This proves byte/error/read-order equivalence in the tested cases; no speedup or RSS claim is made. Combined validation:715 library tests/four ignored,44 public checks, strict all-target Clippy, Cargo formatting,441 included Rust files, inventory and supply policy. Evidence:`fixtures/reference_fidelity/main-pin-fresh-cold-core-integration.json`. Original handoff follows.

# Per-call WAL blob pin reuse

Source-level hotspot: ActiveServiceWalV1::prepare initially validates/hashes every new blob, then recomputes those same immutable blob hashes repeatedly for every reference membership lookup. The live4097 action test performs three actual barriers per action and repeatedly supplies the origin archive/observations, making this O(references*blobs) pure hashing. The separate prototype changes only that membership calculation to reuse its initially computed pins. No hash is trusted across calls, cached file, source state, disk validation, reference read/order, actual sync, mutation or raw bytes change.

Apply prepare_wal to current source_main_active_service_wal.rs. Optional prepare_test_includes adds the exact old-body reference/cfg test module; prepare_test_read_trace adds a cfg(test)-only existing-read observer (no production visibility/wire change). Copy two src leaves. prepared files are evidence, not whole-file replacements. Source Current049b/69cc freezes stay immutable.

Eighteen copied actual Journal/WAL tests and strictClippy PASS; three new controls compare exact sealed Before/Post/record/blobs/references/owner, actual synced Post and all raw content; invalid-new-blob/missing-reference errors; repeated existing-reference read order and first corrupt-vs-missing error precedence. New invalid data still fails before any reference reads. The same actual writer is used for reference and optimized prepare; no financial owner/stub exists. The existing writer-byte golden against actual immutable provider also passes. Parent owns actual Core validation/Root edits/commits. The running4097 baseline was not stopped or modified.

Other live costs remain: actual financial-state encoding/commitment, native complete audit/read views, full current-log trace scans, blob opens/hashes and real fsyncs. No proposal removes those checks or shortens4097. This is a byte-preserving CPU optimization, not evidence that the long test has completed.
