# Current V9 compatibility for V10/Main0

## Root integration status

Fresh Rust1.90 Root builds passed46 focused executions across11 targets,
including both numeric references, actual Main0, all16 mixed native subsets,
the inherited Main routes, and independent V8/V9/V10 byte/refusal gates.
All-target Clippy, formatting and14 genuine source-regeneration checks pass.
Main0 audit snapshots are nonmutating, as documented in
`SOURCE_MAIN_V10_OBSERVER_STORAGE.md`; actual JS bodies and financial outcomes
are unchanged. A populated pre-V11 snapshot has explicit focused-debug
provenance and makes no release claim. The complete release suite is a separate
gate; the last verified full baseline remains V9 b02a045. Source inputs and
state hashes do not establish chain or actor authority.

## Historical work-stage handoff

This work-only revision composes the immutable V10/Main0 candidate over Root's current V9 implementation. It preserves every frozen source corpus and the original `953d91bf` overlay. Root owns actual V10 compilation, runtime validation, mounting and commit.

The old candidate predates the final raw20 fallback implementation, new raw21 withdrawal save and two late V9 fixes. Copying its historical previews would replace verified code. Its runtime transform also assumes the old DB-read callback anchor and no longer composes against current Root.

The revised pure transforms retain the exact current callback sequence and insert V10 activation processing before it. DB factories retain withdrawal-save, threshold-read, transfer-balance, spot receiver and send-read order. DB operations retain threshold fallback-read and transfer-balance order after the new activation hook. Every current include remains present.

V10 explicitly inherits raw21 deferred persistence, the complete actual-owner raw20 fallback helper, raw22 read scheduling, deferred Tally jobs, ticking after a rejected awaited channel write, end-of-Main rejection notifications, upper Main admission, actual unavailable-dependency classification and the canonical input-map artifact namespace. Changed gates use the existing spot capability helper, whose V10 composition admits V9 and V10. Raw0 admission remains exact V10. No default-policy change, earlier-policy widening or witness promotion is made.

The current threshold/transfer/withdrawal helper bodies, actual owners, captured dependency facts, aliases, errors and replay structures are retained. The activation implementation remains the immutable reviewed mixed-native module plus its existing Consensus/Insurance corrections.

Eight inherited suites are copied from current Root: basic Main, lower Main, spot Main, upper Main, transfer schedule, extended transfer continuations, threshold failures and withdrawal failures. Only execution policy, policy labels and version expectations change. Existing V9 tests remain untouched. Main0's stale V9 diagnostic label is corrected to V10; its source observations and earlier-policy refusal test are unchanged. Main0 tests additionally require inherited threshold and withdrawal jobs to drain.

Main0 source evidence remains 105 frames: 43 available Main comparisons, 32 actual Property support calls, one isolated missing-capability refusal and 29 resets. Its three tests, eight inherited suites and eight original V10/review tests give 19 staged tests. No V10 runtime pass is claimed before Root executes them.

Root's `work/prepare-v9-fixture-generator.py` / `work/freeze-source-v9.rs` is authoritative for the old V9 release capture. It retains Mainspot state/effect/replay comparisons through ten source calls, adds raw0/raw10 and new Logic capability refusals, then restores/re-encodes the checkpoint. Compile it against the validated pinned V9 release library before V10 mounting, with the exhaustive policy1–9 provenance guard. Run from `go` for its relative output path and set `CARGO_MANIFEST_DIR` to the checkout.

After that capture, preserve exact head/length/bytes and library provenance. The new library's version gate must restore V9 identically; preserve its raw0 and new bootstrap-capability refusals atomically; reject repaired-CRC relabeling to V8/V10 under the old head; retain default V2; and keep V9 refused by TLSW2/TLSW3. V1–V8 fixtures remain unchanged. Review-only debug captures are excluded from this overlay and must not be promoted as release fixtures.

Apply `prepare_runtime(prepare_policy(current_runtime))` first. Other current files use `prepare_replay`, `prepare_main`, `prepare_fee`, `prepare_accounting`, `prepare_send`, `prepare_channel`, `prepare_transfer`, `prepare_token_channel`, `prepare_withdrawal` and `prepare_proof`. Apply `prepare_activation` only to the frozen mixed-native module; its hash helper is unchanged. Previews are review aids: apply pure transforms to fresh Root. V11 starts after the complete V10 layer.

Rustfmt 1.90 parses/formats the previews and tests. Static audits bind inputs/immutable manifests and verify callback order and includes. No Root write or library build ran here. Actual V10 parity, mixed raw0/nonempty-job behavior, release gates and the real V9 release snapshot remain Root gates. No authentication, arbitrary callback semantics or complete block proof is inferred from hashes or these source comparisons.
