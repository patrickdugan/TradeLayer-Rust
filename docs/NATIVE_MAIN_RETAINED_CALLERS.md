# Retained Main caller component

This is a new source-only moved-owner component. The first bounded milestone
implements Main.Initialize/InitOrLoad/Sync caller frames through the real
TxIndex Initialize and ProcessBlock/SaveMaxHeight children. It does not publish
a Main Session head, provide an owner release, integrate durable caller WAL,
recover ambiguous native writes, or complete a ConsensusReady handoff.

Apply pure functions from prepare.py to CURRENT source text, in this order:

1. txindex_source_owner.rs: prepare_owner.
2. txindex_inline_continuation_tests.rs: prepare_owner_tests.
3. source_main_inline_index_runtime_impl.rs: prepare_inline_runtime.
4. source_contract_runtime.rs: prepare_runtime.
5. source_main_native_session.rs: prepare_native_session, then
   prepare_native_session_tests (test declaration is at module EOF).

Copy every file under src/ to the repository src/. The exact original
fixtures/native_main_inline_callers.jsonl is already committed; the package's
copy must match it. No other source fixtures change. prepare_component.py
performs those overlays only under this stage and records input/output hashes.
Its full-file prepared outputs are review artifacts, not replacements for a
newer Root. The underlying Main initializer prefix is derived from the actual
source_main_service_initialize body; getInstance alone is routed to its actual
native index. The original default method is untouched.

Production public entry:

    SourceMainNativeSessionV13::into_retained_main_caller(
        expected, command, caller_scope, generation, raw_rpc_documents)

The consumed Session remains private, including on an invocation-start error.
Only a pre-source rejection returns the unchanged original owner. No new Replay,
Ledger, financial mirror, expected-state loader, mutable index accessor, head
publication or checkpoint export is added. All financial effects remain in the
actual Core, including the existing capacity flag.

The source component exposes advance, caller_result, next_timer/fire_timer,
pending_descriptor/write, append_pending, resolve_post and complete canonical
observations. Each append and resolve requires the exact retained selector:
caller scope + source job + method scope + operation ordinal + Awaited/Detached.
The selector is source control metadata, not an independently usable native
append capability. A caller result cannot select an operation or release its
owner. The first save write and indexExists write retain distinct ordinals.
An applied write stays held until actual Post verification and source resolve.

The former live inline frame layout, complete visitor and effect adapter remain
byte-exact. Three existing method signatures are retained; their real bodies
are factored over borrowed parts. Advance's completed source evaluation is
consumed at the old wrapper exactly as before. The new extractor consumes its
own actual child, never a supplied result. Mode4 still selects ONLY its direct
live-block field. It cannot fall through to these new caller children.

Checks completed here: four real SourceOwner/Journal extractor controls; all
123 copied source+metadata tests pass, one ignored, Rust1.90 strict Clippy.
Four new actual-Core caller tests are parsed/formatted but NOT compiled here.
Parent should run retained_caller_tests and the four retained_extract_tests,
then the existing seven inline service and six Mode4 controls plus Clippy.

The pending ConsensusReady frame retains the SAME invocation and, when present,
the separate detached extractor child. CallerReturned applies to completed
Sync/Init callers; Initialize has no result before its genuine consensus child
fulfills. The next composition must lend the same invocation to actual catchup,
retain the detached source job independently through that fulfillment, and add
explicit caller/WAL selector admission. No unknown/concurrent source operation
is converted into a caught source Error.

## Actual Root validation

The four new caller and four extractor controls pass in the actual Core. Seven existing inline-service and six Mode4 controls also pass. The combined library passed504 tests (four child entries ignored), and44 public compatibility checks passed. Root then removed only two unused test-only JSON helpers flagged by strict Clippy; no runtime body, comparison, fixture or assertion changed. All eight new controls reran on the final tree, with all-target warnings-denied Clippy, Cargo fmt,279 included-file formatting and route inventory passing. Initial lint diagnostics and exact before/after hashes are preserved. Counts overlap.

The explicit caller/job/method/ordinal/await-role selector binds every append/resolve. The actual caller outcome and detached native work remain separate, and no original owner/Session/head is released. ConsensusReady remains unfulfilled: full Initialize/consensus completion, concurrent publication support and caller WAL/recovery integration are follow-ons. The12 original source calls/35records are not claimed as12 complete Rust matches. Full all-target baseline remains2,142 Rust/239 locked numeric checks. Verification: `fixtures/reference_fidelity/retained-main-caller-core-integration.json`.
