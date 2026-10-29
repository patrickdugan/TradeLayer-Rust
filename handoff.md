# TradeLayer-Rust release handoff

Updated: 2026-10-29T20:27:23Z. This file is maintained after every integration/PR cycle.

Latest full release: **2,120 Rust executions /238 locked numeric checks**, implementation `239c340`, tested `216377f`;470library/44scoped public checks. Evidence:`fixtures/reference_fidelity/native-inline-release-validation.json`. Full reference parity, authentication and complete-block fraud proofs remain open.

## Objective and operating rules

Finish the 48-module JavaScript port and its integration gates. Preserve the pinned JavaScript behavior, with repairs and policy differences named explicitly. Include the fee and reward PRs; their published scope is standalone accounting, with no implicit consensus-policy change or payout authority.

Commit completed, checked local work. Do not push, publish, create a PR or merge without explicit authorization. Keep one actual financial owner and the actual native index. Never import expected financial results, retry an entire Main command to recover a write, or treat a checksum as authentication. UTXORef is a separate consumer of TL state-oracle data; optional Binohash belongs there.

## Verified checkpoints

- Cycle parent commit: `0edb092dc4814ae24724130a726410400a886b23` (completeness refresh).
- Previous committed implementation: `eff87dd817dc3ec7ac3403ecb020b0156ccbedef` (typed control-Store recovery).
- Current full release: implementation `239c3403f05a4b4edef391d40145ed09052ee672`, tested `216377fe9fb0ce89c92a47776e71e2c73a4b03d4`: **2,120 Rust executions and238 locked numeric-reference checks**,281targets and1,776unchanged frozen inputs. Evidence:`fixtures/reference_fidelity/native-inline-release-validation.json`. Prior21016cd/e99ef48 and963aae7 baselines remain preserved.
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

## Tracker checkpoint after WAL/history integration

Saved workbook and sidebar now match code8ce6ad0. The visible release baseline is1,977 Rust/238 numeric checks; the later scoped library result is354. Fourteen intentional cell changes preserve all37 transaction rows, formulas, styles, filters, layout and open proof/parity gates. Evidence: `fixtures/reference_fidelity/wal-history-completeness.json`.

## Native recovery, real exits and cold raw-row cycle

Actual same-owner Before/Intermediate/Post recovery and durable supersession now pass six Core controls. Completed raw ProcessRows historical reexecution passes five source-owner and four metadata controls; independent review found no actionable defect in that bounded scope. One parent matrix runs four actual child exits and validates stored prefixes/blobs. No source action is rerun to recover a same-process write.

Final combined library:370passed/fourignored; public compatibility:25passed; Clippy warnings denied, Cargo fmt,200include formatting, inventory and supply-chain pass. Evidence: `fixtures/reference_fidelity/native-recovery-cold-rows-core-integration.json`. Last all-target release stays1,977 Rust/238 numeric checks.

Root corrections preserve strict assertions: four Err checks avoid a Debug bound on a private financial owner, one unused test mut removed, all four metadata sidecar tests mounted, and the compaction case stops at the first actual primed catchup write instead of consuming its compaction while driving to live. A logically equivalent guard simplification satisfies Clippy. Earlier failures are retained.

Next queued uncompiled candidates: ordered-bootstrap238947 plus source-loadera1f334; cold ACK reader7b839; read-only cold capability4e5bd; generation escrow1869. Native action/catchup/timer and partial-command financial cold replay, escrow wiring and global startup ordering remain open. The three agents continue those actual composition gates.

## Tracker checkpoint after native recovery/cold rows

Saved workbook and sidebar match code77c1b3c: last full release1,977 Rust/238 numeric checks; later scoped library370/public25 and four actual storage exits. Twelve cell changes preserve every transaction row, formula, style, filter, pane and open proof/parity gate. Evidence: fixtures/reference_fidelity/native-recovery-completeness.json.

## Ordered callable bootstrap integration cycle

Integrated238947 +source-loadera1f334 +old-dispatch58cb. All22 Core controls pass;14 source scenes compare98 initializer/caller/notification cutoffs, and both genuine numeric versions reproduce all99 source observations byte-for-byte. Existing wrong-policy/unconfigured step calls retain old outcomes/heads/checkpoint bytes. The new closed profile is outside old TLSW1 witness admission. Pending and notification-pending startup cannot checkpoint/adopt.

Combined library392passed/fourignored,44public compatibility passes, all-target Clippy, Cargo fmt,206include format files, inventory and supply-chain pass. Evidence: `fixtures/reference_fidelity/ordered-bootstrap-core-integration.json`. First run had14 fixture-reader setup failures; the existing deeper fixture parser corrected those without production-limit/expectation changes. Global Main/reOrg/IIFE success, arbitrary thenables, rich callbacks and interrupted-startup financial recovery remain open. v2 Promise3b459 is queued/uncompiled.

Scope clarification for77c1b3c: completed same-process native recovery covers explicit VALIDITY AppendPending plans. Inline processBlockData/save-height/startup puts during Advance still need a retained operation PC, per-operation durable intent and source-faithful success/error handling. Guarded refusal is not final parity and Advance/source prefixes must never be rerun as write recovery. Row/source worker has genuine inline caught-rejection/applied-write/lost-return cases; v8_main is implementing that seam.

Next queued packages:6db historical native-await bridge,4e5 cold capability plus confirmed-discard guard739025,1869 actual generation escrow,8c062 guarded escrow wiring,7b839 readonly ACK history,b2ae typed history codec and3b459 Promise v2. Full acknowledged active/catchup/timer/partial financial cold replay, inline native IO recovery, guarded retention, host validation and authenticated complete-block proofs remain required. Last full all-target release stays1,977 Rust/238 numeric checks.

## Tracker checkpoint after ordered bootstrap

The saved workbook and existing sidebar now match tested code `fdfa8ea`: 392 combined library tests, 44 public compatibility checks and 22 ordered-startup Core controls. Both genuine numeric profiles reproduce 14 source scenes/99 observations; Core compares the 98 initializer/caller/notification cutoffs. All 37 transaction rows, proof/parity gates, formulas, effective styles, filters, panes and geometry were preserved in 12 intentional cell changes. Verification: `fixtures/reference_fidelity/ordered-bootstrap-completeness.json`. Full release remains 1,977 Rust/238 locked numeric checks at implementation `21016cd`, tested `e99ef48`; later scoped integration is not full-suite promotion.

## Historical native-await / cold guard / escrow cycle

Integrated frozen4e5 → discard-fix739 → generation-escrow1869 → historical-await6db, including FIRST-statement guards on both escrow IO entries. Actual shared RowPC now parks at source-computed metadata preparation, verifies the recorded full Post/native mutation, then resolves the same await once. Existing completed-row replay uses this bridge. No financial answers are imported; no same-process action retry is introduced. Original live query/update-hook/prepare order and page-only hot writes remain.

All21 new controls pass inside the actual Core library:413 library/four ignored,44 public compatibility passes,15 focused historical passes including3new Core and5prior completed-row cases. Clippy warnings denied, Cargo fmt,218 include format files, route inventory and supply-chain checks pass. Evidence: `fixtures/reference_fidelity/historical-native-await-core-integration.json`; independent older-provider escrow/cursor review: `fixtures/reference_fidelity/native-generation-escrow-independent-review.json`. The first launcher ran outside the repository and failed before compilation; the corrected launcher and all compiled controls pass, with the initial log retained. No source/test corrections were needed after mounting.

Next:8c guarded active-service escrow WAL wiring,b2ae typed historical pin codec,7b read-only ACK history; row is implementing a sealed original-prefix carrier/full acknowledged-action replay with actual later Post retained, and v8_main is implementing inline processBlockData/save-height/startup source PCs. Promise-v2 candidate3b459 remains queued. Full active/catchup/timer/partial financial cold replay and Windows directory/power-loss validation remain open. Full release baseline remains1,977 Rust/238 numeric checks; this scoped cycle does not promote it. The saved workbook last verified fdfa8ea/392library; a later tracker refresh must compare this cycle separately. No push/PR/merge.

## Guarded escrow / read-only ACK / history pin cycle

Integrated frozen 8c062 guarded service escrow, 7b839 read-only acknowledged-history reader and b2ae22 exact metadata-pin codec. Actual copy/permit and EscrowBoundIntent sync now precede cross-publication native IO, including replacement plans. Lost returns keep the same frame/permit and native-only retry route. No-copy ordinary appends and existing mode bytes remain. The ACK reader retains the real file lock and verifies full contiguous required blob closure; strict pins match actual production writer bytes and still require real disk validation.

All 23 new controls pass in the actual Core suite: 436 library/four ignored, 44 public compatibility, Clippy warnings denied, Cargo fmt, 234 include format files, inventory and supply-chain pass. Evidence: `fixtures/reference_fidelity/native-escrow-ack-codec-core-integration.json`. Root corrected one missing cfg(test) commit_probe: None initializer in the composed read-only WAL owner; no production semantics or expected result changed. Initial failure log retained.

Boundaries: the ACK reader admits HISTORY-WAL, not an unproven ESCROW-WAL cold closure. The escrow mode blocks inline compaction until actual retained inline PC success/recovery is integrated; guard-only refusal is not final JS parity. Financial cold reexecution, cross-publication source views, process-loss finance and Windows directory/power-loss validation remain open. Current next queue: 4a0900 sealed cold-prefix carrier, row full action scheduler, v8_main inline source continuations, 3b459 Promise v2. Last full release remains 1,977 Rust/238 locked numeric checks. The workbook was last synchronized to fdfa8ea/392 library and must be refreshed separately. No push, PR or merge.

## Tracker checkpoint after native escrow / ACK integration

The saved workbook and existing sidebar match tested code `bad3892`: 436 library tests and 44 public compatibility checks. Both recent cycles are covered, with 21 historical-await/cold-guard controls and 23 escrow/ACK/codec controls counted within the suite. Twelve intentional cells changed; all 37 transaction rows, formulas, effective styles, filters, panes, layout and proof gates are unchanged. Verification: `fixtures/reference_fidelity/escrow-ack-completeness.json`. The independent historical-await review passed 108 metadata tests and two separate controls, with strict Clippy; those counts are not added to the Root library. Review: `fixtures/reference_fidelity/historical-native-await-independent-review.json`. The full all-target release baseline remains 1,977 Rust/238 locked numeric checks.

## Sealed cold-prefix / Promise v2 cycle

Integrated frozen 4a0900 sealed-prefix carrier, 3b459 Promise v2 and 8a9d20 genuine microtask observer. The carrier reconstructs ONE original financial prefix while retaining the actual later Post index/lock/cold capability; it exposes no ordinary stale Session or mutable-owner escape. The original public completed-row API still requires exact current Post. The Promise continuation models actual client assimilation/bridge/outer FIFO jobs, partial checkpoints, multiple catches, shared Error capabilities and once-only notifications on the same Core.

All 18 new controls pass: 454 combined library/four ignored; 44 public compatibility; 37 startup (22 retained+15 new) and three cold-prefix scoped cases. Both genuine numeric versions reproduce new12 scenes/143 observations and unchanged old14 scenes/99 observations. Core compares142 new+98 prior source cutoffs; the two explicit full-module boundary observations remain scoped exclusions. Clippy warnings denied, Cargo fmt,240 include-format files, inventory and supply-chain pass. Evidence: `fixtures/reference_fidelity/cold-prefix-promises-core-integration.json`.

Root corrections are test-only: exact metadata-error matches without adding PartialEq to production; inherited UnknownDependency reconfiguration fence; and separate existing-row conflict/fresh-ID guarded-write inputs. Financial/pin/lock/IO assertions remain. All three initial failure logs are retained. No source projection/result was changed.

Next: row full acknowledged-action driver and historical catchup/service backend; v8_main retained inline processBlockData/save-height/startup source PCs and service/WAL recovery. Cross-publication source views, full ESCROW-WAL cold proof closure, killed-process financial recovery, broader startup/callback domains, host validation and later full-suite promotion remain open. Spreadsheet last verified `bad3892`/436 library and is separately maintained; no claim that this newer cycle was already exported. No push/PR/merge.

## Retained inline source/live-service cycle

Integrated6faada source owner/database/service PCs +546690 owner-move preservation +two independent fb9e8a prefix controls. Explicit into_inline_active_service keeps the same financial owner/index/queue while actual processBlockData transaction upsert, MaxHeight and indexExists writes each remain separate native Append/SourceResolve awaits. Decoder/RPC/source prefixes are not repeated. The original entry defaults false and retains old bytes/domains. Save-height/genesis PCs are implemented and helper-tested; their complete Main startup/catchup integration remains open.

All16 new controls pass within470library/fourignored:four owner,three metadata,seven actual inline service andtwo independent prefix regressions.44public compatibility, all-target Clippy, Cargo fmt,250include format files, inventory and supply-chain pass. Both genuine numeric versions execute13original source calls/38records with d492fdfc bytes. Source evidence is not13fullMain Rust parity claims. Current shared service loader/observer remain unchanged; new observer filename plumbing reuses the exact pinned old loader. Evidence:`fixtures/reference_fidelity/retained-inline-service-core-integration.json`; cold-prefix review:`fixtures/reference_fidelity/sealed-cold-prefix-independent-review.json`; inline independent review:`fixtures/reference_fidelity/retained-inline-service-independent-review.json` (123 metadata/owner plusfour independent controls, separate from Root count).

Root test correction was matches! for two independent metadata-error assertions, preserving exact ColdWriteRefused variants. A redundant string-format closure was equivalently replaced with its function for Clippy; final suites were rerun. Initial compile/lint failures are retained. No source behavior, expected fixture or production error type changed. Next: v8_main new immutable inline WAL origin/history binding and actual native ambiguity recovery; row full acknowledged action/catchup source backend and later historical inline playback. Full startup/catchup callers, cross-publication views, killed-process financial recovery, host validation and authenticated complete-block proofs remain open. Spreadsheet last verified bad3892/436library and remains a separately recorded checkpoint. No push/PR/merge.

## Tracker checkpoint after retained inline service

Saved workbook and existing sidebar now match tested code `239c340`:470 library/44 public checks,7 actual inline live-service and5 sealed-prefix scoped checks. Original JS13 calls/38 records per genuine numeric profile are source evidence, not13 full Rust Main caller matches. Twelve original cell addresses changed, with a text readability refinement inside those same cells; all37 transaction rows, formulas, effective styles, filters, panes, geometry and proof gates are preserved. Verification:`fixtures/reference_fidelity/retained-inline-completeness.json`. Full all-target baseline remains1,977 Rust/238 locked numeric checks; later full-suite promotion remains separate.

## Frozen full-release promotion after native inline integration

All281release targets, doc checks and exact86-target locked numeric list pass. Counts:2,119top-level +one successful storage subprocess =2,120 Rust executions;238 numeric checks. These are executions, not2,120 unique source behaviors. All1,776input bytes and checkout216377f were unchanged before/after. Raw execution-manifest SHA and canonical LF checked-in manifest SHA are recorded separately with identical parsed rows. No source change was needed for this full run.

Covered bounded scopes: actual active-service WAL/history/explicit validity native recovery, completed historical rows and sealed earlier prefix, strict ACK/pin readers, guarded generation escrow, ordered Promise continuations and explicit retained inline live source service. This does not establish inline durable recovery/new WAL mode, full acknowledged action or startup/catchup/historical inline, cross-publication financial replay, killed-process finance, Windows directory/power-loss or authentication/complete-block proofs. Fee/reward economic and matcher/payout wiring stays separate.

New e0cee8 cold-action candidate and forthcoming inline-WAL binding were kept OUTSIDE this frozen checkout; they still require actual Core gates and later promotion. The saved workbook/sidebar must separately update the full-release baseline from1,977 to2,120 while preserving all transaction/proof rows.

## Full-release workbook promotion

The saved workbook and existing sidebar now show implementation `239c340`, tested `216377f`: **2,120 Rust executions /238 locked numeric checks**,281targets and1,776unchanged frozen inputs. Sixteen intentional cells updated the release header/count and bounded source/storage descriptions. All37 transaction rows, formulas, effective styles, filters, panes, geometry and parity/proof gates are preserved, with zero formula errors. Verification:`fixtures/reference_fidelity/native-inline-release-completeness.json`. The e0 cold-action and inline-WAL follow-ons remain outside this release.

## CI native test temp default

A test-only fix removes the undocumented requirement to set TL_NATIVE_MAIN_TMP for native_main_integrated. An explicit override is still respected; without one, the test uses the standard temporary folder. Canonical parent validation, unique child names and guarded cleanup remain intact. The exact old missing-variable failure was reproduced; three independent filesystem controls and all seven actual native release tests pass with that override removed. Clippy warnings denied and formatting pass. Production source is unchanged and the 2,120 Rust/238 numeric full-release baseline remains the separate earlier verified cut. Evidence:`fixtures/reference_fidelity/native-main-test-temp-default.json`. No push/PR/merge.
