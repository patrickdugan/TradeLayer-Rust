# Retained inline TxIndex source awaits

This candidate replaces the missing continuation seam, rather than treating the existing before-compaction safety refusal as source parity. It is additive to Root `bad3892c` and the integrated escrow/cold-write guards. It changes no Runtime policy enum, default V2, V1–V14 replay reader, legacy source owner field, old `SourceCompactIndex.database/put` body, old active-service entry, or old active-service observation fields.

The explicit new entry is `SourceMainNativeSessionV13::into_inline_active_service`. Its session/head domain is `TL-MAIN-NATIVE-INLINE-ACTIVE-SERVICE-13-1`. The original `into_active_service` passes false to a factored constructor and retains its original head domain and field stream. This running component has no checkpoint importer, mutable financial/index escape or completed-owner release. The new live-block continuation holds the same outer invocation and borrowed queue while its real TxIndex DB Promise is pending.

## Actual source PCs

`TxIndexInlineContinuationV1` retains the real dependency frame, input arguments, actual receiver-state guard, scope, block/height/transaction cursor, computed rows, original database handle, separate save-height database handle and pending call. It reuses the existing actual `decode`, `transaction`, sender/output and unawaited Types bodies. It does not call `evaluate` again after suspension.

* `processBlockData`: original factory and transaction processing; push computed result before await; suspend the original transaction upsert; source-catch its rejection; obtain the separate save-height factory; suspend MaxHeight then indexExists; advance to the next transaction only after those returns.
* `saveMaxHeight`: null/undefined early return, factory, first update, second update. Failure of either height update propagates with the real earlier write retained.
* `initializeIndex` / instance `initializeOrLoadDB`: original factory/find and optional genesis insert. The initializer itself rejects on insert failure. The distinct Main callers retain their different catch boundaries; they are not merged here.

The live service integrates the first path. Direct save-height and genesis PC controls are implemented/tested at the owner seam, but their complete Main startup/catchup callers are still a follow-on. The original service block Persistence/reorg/header/checkpoint prefix executes once before the new index PC. Its result then uses the existing funding/trade/settlement PCs.

## Real native operation

`NativePreparedInlineDatabaseV1` computes actual memory-copy/unique-ID/update semantics and existing page/directory mutations from the same complete native snapshot. Preparation performs no write. Source exceptions such as duplicate insertion are actual locally calculated DB outcomes, not oracle results.

`append_inline_database` checks the live-write capability before IO, verifies the exact Before state and actual owner-bound append plan, and requires the real sealed escrow permit if that append crosses publication. It changes actual storage only. `resolve_inline_database_post` independently verifies the complete Post and directory before allowing the same source PC to resume. Applying bytes never implicitly settles the source Promise. The actual transaction upsert and the two save-height updates remain three distinct publications.

The retained source frame and native plan have complete streaming field visitors. Inputs/rows/calls/returns, numeric bits, source/local errors, cursor and all prepared mutations are bound without aggregating an entire index into a SourceValue array. Local/resource and storage stops retain the actual frame. Public source-value observations confer no authentication.

## Evidence and limits

The immutable original-JS evidence is row worker's `8163a7f7` package, fixture SHA-256 `d492fdfcb2731f46417556903d7d024e74eb9c7682d4b115795f74f0561bdff9`: thirteen actual calls and thirty-eight input/observation records per genuine BN profile. It proves raw-row rejection is caught before height writes, first/second height partial writes, pending applied returns and distinct static/instance Main startup catches. Main full-module IIFEs are outside that callable-class source harness.

Seven new standalone controls execute the actual copied source owner and real native Journal against the immutable `ad67f3d7` provider's types: full source-evaluation/dependency/native-row equality with the existing owner, exact once-only decode across held writes, metadata preparation/no-IO, separate applied/resolve phases, duplicate insertion and actual escrow-protected compaction/cold-write refusal. The copied metadata suite is also run. These are helper/metadata checks; they are not whole-Core or complete Main source-parity claims.

Seven proposed Core tests replay the existing unchanged-source service comparisons under the new entry and add direct three-await/held-return/Shutdown controls. They are UNCOMPILED by this worker. Parent owns Core compilation, compatibility and final commit.

Remaining RC work is explicit: wire the new entry and generic inline plans into a new immutable active WAL origin/history mode; native Before/Intermediate/Post recovery for inline plans; actual retained startup and catchup inline callers; their historical/cold playback; process-loss gates; and any broader deferred-rejection timing outside the exercised source slices. Existing Mode1/Mode2/Mode3 WAL remains on the old entry. The low-level escrow-capable plan is usable, but this direct new service entry does not itself supply an escrow permit for compaction. No refusal or storage stop counts as successful recovery or source parity.

## Parent integration checkpoint

The live retained-entry component is integrated and all seven actual Core controls pass, including the five existing strict original-source service comparisons and the held transaction/MaxHeight/indexExists and Shutdown boundaries. Four source-owner and three real metadata controls also pass in the actual Core library. The full combined library passes470 tests (four ignored child entries),44public compatibility checks pass, and Clippy warnings denied, Cargo fmt,250 include-format files, route inventory and supply-chain pass.

Both genuine numeric profiles reproduce all thirteen original-JavaScript calls and38input/observation records byte-for-byte. The original thirteen-call corpus is source evidence; this does not claim that all thirteen full Main callers have matching Rust continuations. Direct save-height/genesis PCs are tested at the owner seam; complete Main startup/catchup callers remain open. Isolated original observer/base filenames preserve the current shared service scripts. Source bodies/expected fixture bytes are unchanged.

An additive move overlay retains the new inline-profile flag in the existing no-index recovery carrier and exact owner moves; all old entries default false. The two independent cold-prefix reviewer controls now execute and pass: a nonempty original financial prefix before a later actual native write, and a repaired recorded-receipt mismatch after real source/effect execution retaining the actual Post/capability/lock. Test-only exact error checks use matches!, without changing production types or refusal variants. Root also replaced one redundant string-formatting closure with the identical function for Clippy; final Core/library/public checks were rerun after that equivalent cleanup.

Independent review passed123 actual metadata/owner tests andfour extra boundary controls; those separate counts are not added to the Root library. Review: `fixtures/reference_fidelity/retained-inline-service-independent-review.json`.

This is still not a new durable inline WAL mode, Before/Intermediate/Post inline ambiguity recovery, full inline startup/catchup integration, historical inline playback or process-loss financial restoration. Existing durable modes still use the old entry. No local refusal is counted as successful source parity.
