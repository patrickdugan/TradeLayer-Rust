# Historical inline database preparation

This is the next metadata-only prerequisite after copied-history freeze `45a42cf811891c51a51c0c08d034def23f0af89373fc93eda629e29b893f06f5`. It leaves cold service readers/admission, financial/source owner frames, current live append permits and policy formats unchanged. Parent owns the whole-Core gate and commit.

`VerifiedOriginalMetadataViewV1::prepare_historical_inline_database(method, rawArguments)` computes the currently admitted inline `insertAsync` / exact-ID `updateAsync` operation from actual verified old metadata. The view can be the original actual history or its verified copied generation. Lookup, memory-copy coercion, update shape, source DB outcome, page split/directory mutations and complete-KV receipts use the same existing live algorithms.

The new `NativeHistoricalInlineDatabaseV1` is opaque, non-Clone and distinct from every live plan/token. It contains original history facts, optional separate copy evidence, source-derived raw arguments/outcome, complete Before/After receipts, actual computed directory and exact TLNM bytes. It contains no `NativeAppendPlan`, writer, financial owner or install capability. There is no constructor from a persisted result/plan or expected financial state.

`verify_recorded_native_append(actualPostView, expectedRawTlnm)` compares the independently retained raw mutation blob to the recomputed bytes, then checks actual Post complete KV, exact directory, original namespace/catalog identity/context and one logical operation increment. Only then does it enter `PostVerified`. A separate `resolve_recorded_post()` returns the computed outcome once. It neither appends nor resumes a source PC. Duplicate/no-match/source-reject operations have no native write and return only their independently computed DB outcome; they cannot verify a fictitious native append.

This is a metadata transition check, **not** complete physical native-candidate lineage or Mode4 action closure. Copied identity stays separate from original identity. A future caller must also verify the raw action/ACK chain, exact original native candidate and supersession facts, method/arguments and actual caller/job/scope/operation ordinal/Awaited-or-Detached role. Current cold HISTORY and Mode4 admission remain closed to this extension.

## Pure mount

Requires `45a` already applied. Four new `src` leaves and four pure transforms:

| Existing file | Transform |
| --- | --- |
| `src/source_index_inline_database_impl.rs` | `prepare_inline` |
| `src/source_index_prepared_validity_impl.rs` | `prepare_state` |
| `src/source_compact_index_v2.rs` | `prepare_index` |
| `src/source_index_inline_database_tests.rs` | `prepare_tests` |

The first two transforms factor existing bodies rather than replace a current owner or fake a live index. They verify the exact extracted function/fragment hashes before changing text. `SOURCE_EXTRACTIONS.json` records those original hashes. The live inline preparation retains its original validation/read order and live plan construction; the complete-KV wrapper retains its signature and delegates the identical entry traversal. All other current includes/fields remain in place. `preview/` is hash-bound review output; mount through the pure transforms to preserve any newer Root files.

The new historical canonical visitor uses bounded 16 KiB byte chunks for descriptor, directory and TLNM observations. It retains every byte of actual large pages without constructing oversized hex `SourceValue` fields. The existing live read-context shape/bytes are compared exactly, including raw Undefined/NaN/negative-zero/infinity/Promise inputs and the admitted memory-copy results.

## Actual evidence and scope

Five new standalone metadata controls pass. They compare existing live preparation against historical computation and real Journal writes, independently verify actual pruned copied pages for a split, reject foreign/equal-content physical origins, wrong mutation bytes and later extra state, preserve one-shot verification/resolution, and stream a mutation larger than the 1 MiB source-value limit losslessly in individually valid scalars. The full local metadata/owner closure passes **133 tests, 1 existing child helper ignored**; warnings-denied Clippy passes.

These are helper/metadata tests, not Parent whole-Core tests or new original-JavaScript conformance counts. The package inherits the current source-admitted live metadata slice; it does not claim arbitrary NeDB query/operator support. The unchanged genuine 13-call inline corpus and later caller corpus remain earlier evidence, not new cases counted here. Parent still needs its actual Core compatibility gate. No live/cold action dispatcher, retained caller, recovery-chain reader or financial frame is widened.

Remaining prerequisites include checked Mode4 recovery/ACK interpretation, original/copy generation closure in both cold prefix and action replay, historical metadata callback composition, and explicit caller/job/await-role selection. The actual final extractor `saveMaxHeight` is detached; this metadata API must not be used to reinterpret it as an awaited parent completion.

## Actual Root integration

The five author and two independent controls pass inside the actual Root crate. Exact old-view read contexts/outcomes/TLNM, actual pruned-copy page split, valid-CRC same-size unrelated-state rejection, cold guard/one-shot behavior and large lossless scalar fields are checked. Seven old inline, six Mode4 and six public source-owner controls also pass. This remains a metadata prerequisite; no live plan/permit/financial owner or cold/caller admission is added. The combined library passes511 tests (four child entries ignored); all-target Clippy denies warnings; Cargo fmt,285 included-file formatting, inventory and supply-chain checks pass. Counts overlap. Full all-target baseline remains2,142Rust/239numeric; this later batch is scoped. Verification: `fixtures/reference_fidelity/historical-inline-database-core-integration.json`.
