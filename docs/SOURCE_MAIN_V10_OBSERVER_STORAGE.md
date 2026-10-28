# Main0 nonmutating source observer correction

## Actual failure and cause

After the V10 lazy DB-factory correction, both strict Main0 numeric-profile tests failed at `first10/01-indexed-real-registry-activation`: Rust table index 8 was `consensus`, while the source fixture had `orderBooks`, then `marginMaps`, before `consensus`.

A test-only link against an immutable copy of the actual corrected V10 candidate (Rust 1.90 debug library SHA-256 `f9bba0f557f2f20d4683b8b982363d8ccf966bd71e8887eb0cb3e667445df6ce`) moved the COMPLETE ordered-effect comparison before the inventory assertion. Both profiles passed every ordered DB/RPC/file/postphase/unhandled effect for the first call before failing at the original inventory assertion. No reads, awaits or effects were dropped or reordered.

The discrepancy was the original audit driver, not the pinned business source. `source_contract_execution_vectors.cjs::state()` copied `tables` before building `views`. The view getters used `table('orderBooks')` and `parseRows('marginMaps')`; that helper CREATES absent empty collections. Taking the reset snapshot silently mutated the next call's storage prestate without a business operation or DB effect. The emitted reset table inventory did not include those newly created collections.

## Narrow correction

Only `main_activation_vectors.cjs` composes nonmutating state-view reads: `(tables.get(name) || [])`. Missing collections still produce empty diagnostic views. A snapshot regression assertion requires the table-name sequence before and after the audit snapshot to be identical. The pinned module/class method bytes, operations, adapters, returned data, financial fields, validators, and scheduling are unchanged. No Core/runtime change is made for this issue. Existing V1–V9 generators, corpora and replay bytes are untouched.

The complete 105-frame corpus was regenerated. Four checks pass: full/default, full/locked, proof10/default, proof10/locked. Counts remain 29 resets, 43 available Main calls, 32 actual Property support calls, and one isolated unavailable Main call (44 actual Main calls total). Proof10 remains the original first ten actual Main calls, with its three reset frames.

The observer-difference audit checks exact tagged transport: every command, result, summary, ordered effect, and every state field outside `source.tables` is identical. All persisted rows for every collection are identical. Only empty collection inventory/order differs in 76 frames; 29 inventories remain unchanged. Source/class/method hashes, findings and dependency metadata are identical except the explicit observer note and new fixture hash. No table sorting, table filtering, state projection, or assertion relaxation is applied to the Rust tests.

The unchanged strict Rust Main0 test, copied byte-identically from Root, passes all three tests against the pinned corrected V10 library after regeneration, including both numeric profiles and old V8/V9 raw0 refusals. Test-only Clippy with `-D warnings` passes. Parent still owns the actual Cargo gate and the subsequent V10 debug fixture capture.

## Mount

Apply `prepare.py::prepare_main0_observer` to the latest Root generator or copy the identical reviewed script. Mount only that script, the three regenerated fixture/inventory files, and this document. `tests/source_main_activation_callable.rs` and `scripts/main_activation_observer.cjs` in this WORK stage are byte-identical comparison inputs, not replacement payloads. `main_effects_first.rs` is only a diagnostic that preserves strict comparisons while changing their order; do not mount it.
