# TradeLayer-Rust release handoff

Updated: 2026-10-29T16:32:02Z. This file is maintained after every integration/PR cycle.

## Objective and operating rules

Finish the 48-module JavaScript port and its integration gates. Preserve the pinned JavaScript behavior, with repairs and policy differences named explicitly. Include the fee and reward PRs; their published scope is standalone accounting, with no implicit consensus-policy change or payout authority.

Commit completed, checked local work. Do not push, publish, create a PR or merge without explicit authorization. Keep one actual financial owner and the actual native index. Never import expected financial results, retry an entire Main command to recover a write, or treat a checksum as authentication. UTXORef is a separate consumer of TL state-oracle data; optional Binohash belongs there.

## Verified checkpoints

- Cycle parent commit: `0edb092dc4814ae24724130a726410400a886b23` (completeness refresh).
- Previous committed implementation: `eff87dd817dc3ec7ac3403ecb020b0156ccbedef` (typed control-Store recovery).
- Current full release: implementation `21016cd777906cc8dc9ce8fc6fa7ec8e33aac351`, tested checkout `e99ef482d0f258d19a7a10aa776e6923d1c22532`: **1,977 Rust executions and 238 locked numeric-reference checks**, 281 targets, all 1,646 frozen inputs unchanged. Evidence: `fixtures/reference_fidelity/rc-cycle-release-validation.json`. Prior `963aae7` baseline remains preserved.
- The Main cycle is **committed at `bbe36ddc953d6a2fb78964127a4562cba805c808`**. Its exact source/check provenance is `fixtures/reference_fidelity/native-main-service-publication-core-integration.json`. Inspect `git log -1` for the integration tip; the workspace mount ledger is `../../work/root-native-service-publication-mounted.json`.

## Current Main cycle

Committed in the Main cycle: actual completed-command receipts/history/head publication, retained-owner final Accepted/ACK recovery, explicit running-service continuations, actual live block/row execution and real same-index TxUtils bootstrap.

The final Store/publication/controller gates pass **32 distinct controls**:

- 12 Store controls: final Accepted/ACK preparation, original bytes, exact healthy retries, synced-Post rebind, empty/no-native-write commands and corruption refusal.
- 20 session controls: original receipt-byte parity, next command, both writer modes, retained original Before, native/control reopen, final-write ambiguity and one guarded effect drain.

Clean scoped logs: `../../work/root-native-final-store-scoped.log` and `../../work/root-native-final-controller-scoped.log`. Post-controller all-target Clippy passes in `../../work/root-native-final-controller-clippy-first.log`.

The corrected service gate passes **11 controls**: all eight service/block comparisons and three explicit-client-scope controls. All original table/native-row and selected ordered-effect assertions remain strict. The new scope shares actual client counters/failure facts and closes on return/park/unwind; ordinary V12/native/service callers retain their old behavior. The raw height and RegExp test transports preserve declared input and flags.

The combined library passes **318 tests with eight threads**, with three child entries ignored. All **25 public compatibility checks**, all-target Clippy, Cargo fmt, **180 included-file formatting checks**, supply-chain and transaction inventory pass. This is 386 successful promotion-gate executions (43 scoped +318 library +25 public), with overlapping tests. The full-release baseline remains unchanged.

## Completed proposal provenance

The Main, fee and reward proposals below have already been integrated and checked. Their original immutable manifests remain provenance; do not apply those proposals again. Parent owns Root edits, whole-Core Cargo, integration and commits. Agents work in isolated stages and return new immutable manifests.

## Fee PR cycle #152

Integrated pinned PR head `6b9e43b00fad76b9810386fbaaf4da3e372f07f9` with the confirmed forged-total defect corrected. `FillFees::new/validate` enforces checked signed accounting identity; accrual rejects inconsistent/negative/overflowed totals before any mutation. All **30 actual current-Core integration tests**, all-target Clippy, formatting and transaction-inventory checks pass. Provenance and logs: `fixtures/reference_fidelity/pr152-fees-core-integration.json`.

This cycle adds the PR's standalone arithmetic/accounting API. Matcher/ledger/consensus wiring and fee-policy choices are separate work, and no payout authority is inferred from a fee record. Reward PR #153 is now integrated in the following cycle.

## Reward PR cycle #153

Integrated pinned PR head `3a1cfc4eeffb7dcf73ece1b120a912f8848ff294`. Production arithmetic is unchanged. The randomized check now verifies the complete 256-bit remainder, and **1,072 independent Python full-width vectors** cover products, quotients, zero divisors and overflow. All **9 unit +21 current-Core public-API tests**, all-target Clippy, formatting and transaction-inventory checks pass. Tests include 100,000 seeded epochs and 20,000 identity splits. Provenance/checks: `fixtures/reference_fidelity/pr153-rewards-core-integration.json`.

The accounting bound uses declared fee inputs; maker rebates/refunds can reduce actual cost. Tests/documentation retain the gross-fee counterexample and the independently netted-cost example. Per-account caps do not establish identity. No new economic parameters, consensus policy, mint or payout path was chosen.

## Active next-cycle lanes

- `/root/v8_main`: actual-owner active-service WAL and ambiguous control-write recovery, coordinated raw history contract.
- `/root/row_continuation_impl`: verified actual-file historical metadata views, then cold financial/source reexecution with physical/logical bindings kept explicit.
- `/root/v8_clearlist`: original import/static-init ordering and rejected-bootstrap notification parity using the 14-scene/99-observation source corpus.
- Parent owns current branch edits, actual Core/release checks, commits and this live handoff. Prior fee/reward candidate roles above describe their completed integration cycles.

## Remaining release gates

1. Current Main/fee/reward release gate passes 1,977 Rust executions and238 locked numeric checks. New recovery candidates below are not covered by that run.
2. Fee/reward matcher/Ledger wiring and economic/consensus choices remain separate from their tested standalone API scope.
3. Durable active-service action/WAL composition and cold process-loss replay. A fulfilled outer promise can still own pending live work and cannot release a frozen head.
4. Global import/static initialization order and deferred bootstrap rejection handling. New source corpus `ecc021cc` has 14 scenes/99 observations, both numeric profiles checked; Rust parity is pending. It does not prove the full Main module initialization IIFE.
5. Broader source receiver/cache/history/value domains, full release promotion, intended-host storage/power-loss checks and representative real-block CPU/RSS.
6. Authenticated chain/UTXO/actor inputs and complete-block proof coverage. Current context hashes establish integrity only. Schedule the oracle-abuse/UTXORef fraud-proof exercise after the integration is nearly complete.

The authoritative transaction inventory remains `docs/LOGIC_BY_TX_TYPE.json`: 36 routed types plus audit row 36. Keep remaining gates visible; do not promote an unexecuted proposal or unavailable handoff to complete.

## Incoming PR provenance

- #152: `6b9e43b00fad76b9810386fbaaf4da3e372f07f9`.
- #153: `3a1cfc4eeffb7dcf73ece1b120a912f8848ff294`.
- Broader pinned inventory: `../../work/parallel-pr-146-153-inventory.json`. #151 includes #147 plus one extra commit. Preserve current local ledger/runtime changes when composing older PRs.
- Separate #148 review: simple-send property 28 is valid; channel-commit property 28 collides with the source synthetic prefix. The advertised maximum atomic amount loses one atom in both genuine JavaScript numeric profiles. These are recorded cases, not repaired Root implementation.

## Local check environment

Use Rust `1.90.0-x86_64-pc-windows-msvc` and `../../work/cargo-check.ps1` with named `-CargoArguments @(...)`. Root debug target is `../../work/main-v12-target`; release target is `../../work/source-settlement-target`. Use a dedicated workspace temp directory and preserve distinct per-module test directory prefixes.

The workbook is `../completeness-6b9c6c5f/TradeLayer-completeness.xlsx`; its sidebar mirror must match the saved file. Keep full-release counts separate from repeated or overlapping scoped checks. Refresh from committed code and completed logs after this cycle.

## Tracker checkpoint after fee/reward integration

The saved workbook and sidebar were refreshed against `21016cd` with 12 intentional cell changes. All 37 transaction rows, proof/parity gates, formulas, styles, filters and layout remain unchanged. Verification: `fixtures/reference_fidelity/rc-cycle-completeness.json`. The full release baseline is still 1,714 Rust /238 locked reference checks until the next frozen run completes.

## Release verification cycle

All 281 release targets and the exact 86-target locked numeric-reference list pass. Counts are 1,976 top-level passes plus one successful storage child, not 1,977 unique source behaviors. The first restricted-network attempt stopped on eight client loopback PermissionDenied errors; all 11 client checks then passed with loopback access, and the complete frozen rerun passed without source changes. Failure and rerun hashes are preserved in the release record.

Those two immutable candidates are integrated in the WAL/history cycle below. Their original worker checks remain provenance; actual Core results supersede their uncompiled status.

## Active-service WAL and historical metadata integration cycle

Implemented and checked:19 actual-Core WAL/codec/controller controls and8 actual-file history controls; the combined parallel library passes354 tests (three ignored), all-target Clippy, Cargo fmt,191 include formatting checks, transaction inventory and supply-chain checks. Evidence: `fixtures/reference_fidelity/native-main-wal-history-core-integration.json`.

The actual owner/frame remains held through ambiguous control writes. Exact Before retries only sealed WAL bytes. Historical metadata is read from actual files under the retained current lock; missing compacted history refuses. No old financial state is imported. Native-index ambiguity recovery, cold financial replay and global startup ordering remain open. The last all-target release is1977 Rust/238 numeric checks and does not include this later scoped cycle.

Root corrections to the immutable worker proposal: two Core API call signatures, an allowed test namespace, unique test-directory prefix, and direct shared raw-input parser use to satisfy warnings-denied. Original compile/setup failures and the clean final run remain recorded. Next uncompiled freezes: native recovery b1afc804, historical raw-source rows c05aa394, ordered-bootstrap work in progress.
