# Local checkpoint — 2026-10-28

Travel checkpoint saved, then resumed on 2026-10-28. No push, publication or PR.

## Saved implementation

Opt-in MainCallableV8 / TLCR8 composes the actual Activation registry, callable Main phases, ClearList, send/commit and raw transaction type 3 UTXO-trade ingress with the existing shared Core. Default GlobalClearingV2 is unchanged. Frozen V7 bytes were generated independently from the preceding verified release and preserved with provenance. The unchanged JavaScript observer now records its genuine third Orderbook receiver.

## Validation boundary

The last fully validated baseline remains cd9dd14c68192e77ffd230531505c9fe45ba2585: 1,461 Rust test executions. V8 has focused results only. The earlier focused checks passed ClearList (3), send/commit (4), frozen-V7 compatibility (2), and Main (2, cached Rust 1.90 link). The final five-target Cargo rerun, including raw3, is recorded below after completion. Full V8 suite, Clippy rerun, release build, final inventory pins and spreadsheet promotion remain pending. Do not interpret the previous baseline evidence as validation of this checkpoint.

## Resume order

1. Read work/v8-travel-focused-final.log and this checkpoint result.
2. Complete the broader Main lower-transaction corpus in work/next-main-lower-corpus. Isolate unbound RPC capability experiments in separate resets; do not skip a call while comparing its later mutated Node state.
3. Re-run Clippy, refresh the source inventory pins, execute full/locked/release gates, then update the completeness workbook from those results.
4. Preserve an independently compiled V8 compatibility fixture before mounting the staged SpotLifecycleV9 work.

The Main lower corpus and Spot implementation remain staged under the workspace work directory; they are not admitted or claimed validated here. Prior travel archive is retained. Raw3 preserves the original omitted-validator-output exception; it does not fabricate a successful financial transaction or authentication. No full transaction parity or fraud-proof completion is claimed.

## Final checkpoint checks

Rust 1.90 direct formatting (library, all five new targets and include files) and git diff --check passed. The five-target Cargo run reached a raw3 oracle harness failure: Node 24 rejected the script shebang under -e. The test now strips only that interpreter directive before evaluating the unchanged oracle body. This correction still requires its rerun; no source parity pass is claimed for raw3. All remaining gates above stay pending.

## Resumed validation

At source checkpoint a405348 (with inventory review 901cf01), the raw3 harness correction passes all three focused Rust tests. All 14 new V8 focused tests have now passed. All-target Clippy with warnings denied, pinned formatting and implementation-include formatting pass. The zero-external-crate policy, 11 supply-policy mutation tests and 10 Core bridge Python tests pass. Original Main and raw3 source fixtures regenerate identically under both genuine BigNumber versions.

The full Rust suite remains running in work/v8-resume-full-tests.log, terminal handle 99999; poll that existing handle before starting another build. The long economic_source_catalog_oracle_snapshot group passed all five tests. Full/release validation has not yet been completed.

The broader Main lower integration is frozen under work/next-main-lower-integration with 38 source frames, 23 comparable calls and two isolated unavailable-RPC calls. Its corrected portable observer emits freshly executed records. Parent must verify the latest manifest, mount after the current suite is terminal, and compile/run the new guard and test together. Future spot V9 is frozen in work/next-lower-spot/root-ready; source-only Main+spot and activation work are separate work stages. None is promoted by the V8 focused results.
