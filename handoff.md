# TradeLayer-Rust release handoff

Updated: 2026-10-29T15:25:30Z. This file is maintained after every integration/PR cycle.

## Objective and operating rules

Finish the 48-module JavaScript port and its integration gates. Preserve the pinned JavaScript behavior, with repairs and policy differences named explicitly. Include the fee and reward PRs; their published scope is standalone accounting, with no implicit consensus-policy change or payout authority.

Commit completed, checked local work. Do not push, publish, create a PR or merge without explicit authorization. Keep one actual financial owner and the actual native index. Never import expected financial results, retry an entire Main command to recover a write, or treat a checksum as authentication. UTXORef is a separate consumer of TL state-oracle data; optional Binohash belongs there.

## Verified checkpoints

- Cycle parent commit: `0edb092dc4814ae24724130a726410400a886b23` (completeness refresh).
- Previous committed implementation: `eff87dd817dc3ec7ac3403ecb020b0156ccbedef` (typed control-Store recovery).
- Last full release baseline: `963aae732a1274bb4dafacd24e9d6666451ac51c`: **1,714 Rust executions and 238 locked numeric-reference checks**. Later scoped checks do not replace this baseline.
- The Main cycle is **committed at `bbe36ddc953d6a2fb78964127a4562cba805c808`**. Its exact source/check provenance is `fixtures/reference_fidelity/native-main-service-publication-core-integration.json`. Inspect `git log -1` for the integration tip; the workspace mount ledger is `../../work/root-native-service-publication-mounted.json`.

## Current Main cycle

Implemented in the working tree: actual completed-command receipts/history/head publication, retained-owner final Accepted/ACK recovery, explicit running-service continuations, actual live block/row execution and real same-index TxUtils bootstrap.

The final Store/publication/controller gates pass **32 distinct controls**:

- 12 Store controls: final Accepted/ACK preparation, original bytes, exact healthy retries, synced-Post rebind, empty/no-native-write commands and corruption refusal.
- 20 session controls: original receipt-byte parity, next command, both writer modes, retained original Before, native/control reopen, final-write ambiguity and one guarded effect drain.

Clean scoped logs: `../../work/root-native-final-store-scoped.log` and `../../work/root-native-final-controller-scoped.log`. Post-controller all-target Clippy passes in `../../work/root-native-final-controller-clippy-first.log`.

The corrected service gate passes **11 controls**: all eight service/block comparisons and three explicit-client-scope controls. All original table/native-row and selected ordered-effect assertions remain strict. The new scope shares actual client counters/failure facts and closes on return/park/unwind; ordinary V12/native/service callers retain their old behavior. The raw height and RegExp test transports preserve declared input and flags.

The combined library passes **318 tests with eight threads**, with three child entries ignored. All **25 public compatibility checks**, all-target Clippy, Cargo fmt, **180 included-file formatting checks**, supply-chain and transaction inventory pass. This is 386 successful promotion-gate executions (43 scoped +318 library +25 public), with overlapping tests. The full-release baseline remains unchanged.

## Worker boundaries and pinned proposals

- Main service correction: `/root/v8_main`. `../../work/native-main-service-shared-client-correction/root-ready-shared-client-correction-1/MANIFEST.json`, SHA-256 `eacef4f3e554801054b2c1162cb014d885a18eb748a4a088ad6a89e5004ca42b`. Apply its exact pure hunks to current files; shared whole-file previews are review material.
- Fees PR #152: `/root/row_continuation_impl`. Correct the public `FillFees` inconsistency before integration; accrual currently accepts zero charged fees with a forged positive net. Preserve atomic rejection and signed arithmetic boundaries.
- Rewards PR #153: `/root/v8_clearlist`. Preserve intended accounting. Strengthen full-width arithmetic tests with independent results; the original randomized predicate accepts an incorrect zero quotient. The implementation passed 30 independent full-width vectors; no arithmetic implementation bug was reproduced.
- Parent owns Root edits, whole-Core Cargo, integration and commits. Agents work in separate stages and return immutable manifests. No simultaneous Core builds.

## Fee PR cycle #152

Integrated pinned PR head `6b9e43b00fad76b9810386fbaaf4da3e372f07f9` with the confirmed forged-total defect corrected. `FillFees::new/validate` enforces checked signed accounting identity; accrual rejects inconsistent/negative/overflowed totals before any mutation. All **30 actual current-Core integration tests**, all-target Clippy, formatting and transaction-inventory checks pass. Provenance and logs: `fixtures/reference_fidelity/pr152-fees-core-integration.json`.

This cycle adds the PR's standalone arithmetic/accounting API. Matcher/ledger/consensus wiring and fee-policy choices are separate work, and no payout authority is inferred from a fee record. Reward PR #153 is frozen and next for Root integration.

## Remaining release gates

1. Main cycle is committed; run a new full-release promotion after the feature cycles below.
2. Integrate corrected fees/rewards modules and their actual tests; record their own check scope.
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
