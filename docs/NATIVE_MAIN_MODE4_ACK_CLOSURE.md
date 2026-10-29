# Mode4 physical ACK and inline metadata closure

# Explicit Mode4 physical ACK and inline metadata closure

This additive candidate requires copied-history `45a42cf8` and historical-inline `990224c8`. It adds a **separate** Mode4 metadata reader and independently retained raw commit-trace pin. Ordinary HISTORY open, old WAL writer/pin bytes, default V2 and current financial/source replay admission are unchanged. Root owns whole-Core validation and commits.

## Actual boundaries

`SourceMainNativeDurableActiveServiceV1::capture_inline_cold_pin()` is a readonly explicit checkpoint in the existing Mode4 Idle/ACK scope. It returns `InlineActiveServiceColdPinV1`: existing WAL prefix/location, actual metadata namespace and actual ordered Journal commit-trace receipt. This expectation must be retained independently before cold use. It is neither an append permit nor authentication.

`VerifiedInlineActiveServiceAckHistoryV1::open` accepts only that new profile and the exact existing inline-service origin. It owns the actual readonly WAL lock, checks existing ACK/blob/observation/history links, decodes/reencodes raw actions with the actual existing action codec, verifies the entire actual CRC commit sequence and expected trace, and checks Created → Intent → optional escrow/replacement barriers → Accepted → Ack order. Missing, late, duplicated, reordered or unknown barriers refuse. A CRC-valid rewrite of a past record with unchanged final KV/sequence is still detected by the independent trace; a newly supplied matching trace does not make a malformed barrier graph valid.

The trace is a conditional integrity observation of ordered bytes under the same lock. It is **not** proof of historical fsync, power-loss survival, actor authority or consensus. FileSyncOnly and existing directory-sync limitations retain their original meaning. No current writer schema changes and no trace/copy work is added to an ordinary hot append; capture and verification are explicit readonly operations.

`verify_inline_operation(actualFinalIndex, sequence, actualRecomputedHistoricalOperation)` checks the original source-derived method/arguments/read-context/TLNM against the original native candidates. It requires the actual current metadata owner to equal the final independently bound WAL Post. Old points come from actual original history or a verified copied generation; original identity and copied-file identity are never equated. All retained escrow descriptors must be strict, actual published copies at the origin's configured root/durability, with exact original point/context and an earlier EscrowBoundIntent.

The verifier follows actual referenced recovery blobs, binds Before/Intermediate/Post to the original or replacement candidates, checks real reopened history/full KV, native-only supersession binding, unchanged configuration, exact replacement mutations and their hash, and increasing NativeSupersessionIntent order. Only native metadata preparation is recomputed from the actual reopened old KV; it does not rerun Main, decode, Logic or the action. A final Post is verify-only. No decoded pin is promoted into a writer token or `NativeAppendPlan`.

The returned `VerifiedMode4InlineClosureV1` is metadata-only. The supplied opaque historical operation enters PostVerified only after the complete checks succeed; resolution remains a separate operation. Failed checks leave it unconsumed. There is no financial/source owner returned, imported Accepted/Before execution result, or legacy head escape.

## Scope still closed

This is the current **single-inline service Mode4** scope. The ordinary e0 executor still cannot accept this new reader type. Full raw action/source recomputation, whole before/after source observation comparisons and final source cold replay admission remain separate gates. The new retained caller profiles/kinds and their sealed entry origin, callerScope/sourceJob/methodScope/operationOrdinal/Awaited-or-Detached route are explicitly **not** selected or admitted here. Do not use a first-pending-operation fallback. Native plans whose predicted candidates never occurred are retained WAL expectations, not fabricated actual files; executed/reopened/copy points must verify against actual data.

Discarded-tail counts and historical source error observations are integrity-bound recorded diagnostics. Reading them cannot prove the historical storage device event or turn StorageStopped into source success.

## Pure mount

Seven new leaves and six transforms in `prepare.py`:

| Current Root file | Transform |
| --- | --- |
| `src/storage_history_prefix_impl.rs` | `prepare_history_scan` |
| `src/storage.rs` | `prepare_storage` |
| `src/source_main_active_service_cold_history_impl.rs` | `prepare_ack_reader` |
| `src/source_main_active_service_wal.rs` | `prepare_wal` |
| `src/source_main_native_session.rs` | `prepare_session` |
| `src/source_main_inline_wal_tests.rs` | `prepare_core_tests` |

The storage factor retains the old scanner signature and its noop visitor path; actual CRC/current-suffix/complete-KV validation remains shared. The HISTORY factor retains public `open` with `inline=false`, old entry fields/Debug layout and strict old-profile refusal. New reader state/types are separate. Apply pure transforms to current text; `preview/` only records the reviewed result, preserving later Root includes and retained caller work.

## Checks

Nine new standalone metadata controls pass (three actual Journal trace tests and six actual WAL/index closure tests). The full local metadata/owner closure passes **157 tests, 1 existing child helper ignored**, warnings-denied Clippy passes. The helper links only public support from the independently pinned finished provider `513c8676…`; all new storage, metadata, reader and closure code is local. This is not whole-Core execution.

Administrative WAL fixtures deliberately label their source observations/archive as opaque metadata fixtures; they are **not** genuine financial/source execution cases. They perform actual native Before/Post/Intermediate failures/reopens, real copy publication, WAL barriers, wrong-copy rejection and mutation substitution checks. No new original-JavaScript cases are counted.

Three additional Root-only tests use the existing genuine Main service setup and same retained financial/index/frame owner for ordinary ACK, lost native Post and Before replacement. They are parsed/formatted but **UNCOMPILED / pending Parent**. Their private test-only move releases only the WAL writer for readonly verification; no production owner escape is added and no source await is resumed by the verifier.

Retained failed logs include the first helper compile against an older provider lacking the public raw-action codec and two Windows test observer reads attempted through a second handle while the same Journal lock was held. Tests now inspect those bytes through the actual locked handle; production storage checks were not relaxed.

## Actual Root validation

The combined batch passes25new controls:3actual-source Mode4,9metadata/trace,4actual caller Start,6input-codec and3Store checks. Library545/fourignored,44public compatibility checks, all-target Clippy with warnings denied, Cargo fmt,303included file formatting, route inventory and supply policy pass. Counts overlap. Record: `fixtures/reference_fidelity/mode4-caller-start-core-integration.json`.

Root preserved the failed Core evidence and fixed two narrow issues. Two caller diagnostic encodings now borrow Option<String> as &str. The real Mode4 writer committed an unscoped origin observation while its first Before included the escrow-scope frame, leaving the referenced origin blob absent. Mode4 initial observation now uses the identical shared scope encoder. All existing per-action observation bytes, noninline initial observations and the strict reader checks remain unchanged. Existing affected histories remain refused, rather than fabricating a missing observation.

This closes the named metadata/ACK and Start-only boundaries. Complete raw-action Mode4 cold reexecution, selected caller actions/native recovery/cold caller replay, process-loss financial recovery, host durability and authentication remain open. Full release baseline stays2,142Rust/239numeric.
