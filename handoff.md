# TradeLayer-Rust live handoff

Updated: 2026-10-30T01:38:11Z. Refresh after every tested integration cycle.

## Current checkpoint

The tested implementation is `a94ea4c7cdf4677d7d48455d65363edd263a16c1`. Release evidence was committed as `4a85b00e8a5365e018e5b1f50fb347058e4af473`.

- **2,204 Rust executions:** 2,203 top-level tests plus one successful storage child, across 283 targets.
- **239 genuine BigNumber 9.1.2 checks:** the exact 87-target numeric CI list.
- Library: **549 passed, four ignored**. Doc tests completed with zero cases.
- All **1,888 tracked inputs** matched their frozen hashes before and after the run.
- Pinned Rust 1.90.0; release tests used offline/frozen dependencies. That release also passed all-target Clippy with warnings denied, Cargo formatting, formatting of 304 included Rust files, inventory and supply-policy checks.

Evidence: [release validation](fixtures/reference_fidelity/retained-callers-mode4-release-validation.json). Its adjacent input manifest preserves the frozen file list. The previous 2,142/239 release remains historical evidence.

The saved completeness workbook and existing sidebar agree on this release. Sixteen intentional cell edits preserved the 48 TL module rows, one external reference, all 37 transaction rows, formulas, styles, filters, panes and geometry. Zero formula errors. Evidence: `fixtures/reference_fidelity/retained-callers-mode4-release-completeness.json`.

Later focused correction: rejected append ACKs now preserve the prior original/replacement native relation, while a current API rejection cannot reuse an older StorageStopped result. Five new actual source and three metadata controls pass; library **557/four ignored**, 44 public checks, Clippy/fmt304/inventory/supply pass. Evidence: `mode4-rejected-append-core-integration.json`. The full release remains 2,204/239 until the next complete run.

**This is a passing development checkpoint, not a completed RC.** The source-composition and operational gates below remain open.

## Latest tested composition

The latest focused batch passes **594 library tests (four ignored), 44 public checks, all-target Clippy with warnings denied, Cargo formatting, 338 included-file formatting checks, inventory and supply policy**. Its 37 new unit tests cover selected caller WAL/native recovery, primitive coercive Sync, direct retained file retries and ordinary Mode4 raw-source reexecution through three successive native operations. Counts overlap with the full release; do not add them to it.

The original JavaScript coercive corpus regenerates identically under genuine BigNumber 9.3.1 and9.1.2: 26 calls/78 observations per profile, including the independently checked first10. The source numeric comparisons preserve literal string/number/boolean/undefined behavior and the special primitive string equality branch. Null and object inputs remain outside this explicit profile.

Root corrected a real cold dispatch fault: catchup ResolvePost resumed through the older helper that refused live inline blocks. The new Mode4 path resolves the same row and resumes through its own metadata access; full Before/Accepted comparisons and old HISTORY behavior stay intact. Failed gates are retained. Test-only corrections distinguish child completions from parent fulfillment and preserve the exact recovery-phase assertions after an applied/lost return.

Evidence: `fixtures/reference_fidelity/main-composition-core-integration.json`. The complete release baseline remains **2,204 Rust/239 numeric**, tested at `a94ea4c7`, until another full release run. Automatic Incoming/file/native-tail composition, null/global startup, recovery cold replay, fresh factual live inputs and history rotation remain pending.

## Rules and ownership

Reference JavaScript: `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Preserve literal source behavior, including named source bugs and throws. A repaired protocol needs an explicit policy; do not silently substitute successful behavior.

Keep one actual financial owner, native index and borrowed queue. Expected financial tables, results, validity verdicts, Before/Accepted observations and receipts are comparisons, never execution inputs. Uncertain IO must retain that owner and retry only the exact native/control operation; never repeat the source constructor, Main action, decoder or financial prefix.

Root owns repository edits, checks and commits. Run only one whole-Core Cargo process at a time. Agents use isolated stages and return immutable manifests; apply pure transforms to current files rather than stale whole-file overlays. Commit tested work on the current branch. Never push, publish, open a PR or merge without an explicit request. Preserve unrelated changes. Before any authorized Rust push, run the full preflight required by `AGENTS.md`; Lean/Kani/GitHub-only checks are separate.

TL produces checkable state-oracle data. **UTXORef is a separate consumer repository.** Optional Binohash work belongs there. Hashes, copied history and claimed actors do not establish authentication.

## Integrated scope

All record names below are under `fixtures/reference_fidelity/` and are included by the full release record. Counts overlap; never add focused runs to the release total.

| Component | What the tested code establishes | Evidence |
| --- | --- | --- |
| Native rows and catchup | Actual awaited metadata PCs, financial prefixes, ordered settlement, Before/Intermediate/Post recovery and completed-command publication | Existing native row/catchup/recovery records and release manifest |
| Retained Main callers and consensus | Initialize/InitOrLoad/Sync in the named admitted branches; actual TxIndex extraction; catchup borrows the same invocation. Initialize may fulfill while its detached final save remains held. The original owner/header stays retained. | `retained-main-caller-core-integration.json`, `retained-main-consensus-core-integration.json` |
| Caller Start WAL | Intent is synced before the actual constructor. Ambiguous Accepted/ACK writes retain the executed owner and never repeat construction. StartAcknowledged is distinct from Initialize fulfillment. | `mode4-caller-start-core-integration.json` |
| Mode4 recovery and physical closure | Same-process native-only recovery, synced escrow/supersession, independently retained commit traces, strict Intent/Accepted/ACK ordering, original/copied metadata preparation and exact final Post. The metadata reader leaves the source await unresolved. | `inline-native-recovery-wal-core-integration.json`, `mode4-caller-start-core-integration.json`, `mode4-ack-independent-review.json` |
| Historical metadata | Strict original/copy identities and full KV checks, copied views after actual pruning, recomputed historical inline results/read contexts/TLNM. No live writer permit comes from a decoded pin. | `copied-history-view-core-integration.json`, `historical-inline-database-core-integration.json`, `historical-inline-database-independent-review.json` |
| Ordinary cold source actions | Actual raw-action reexecution on one retained financial/index owner in the older HISTORY/current-original-publication domain. Mode4/caller/recovery domains remain separate. | `cold-acknowledged-actions-core-integration.json` |
| Bitcoin supported transactions | Selected transaction/block/witness binding, six supported input templates, complete supplied funding collection, admitted CheckTransaction rules and value conservation. Parent unspentness, chain validity and TL actor/economic authority remain unestablished. | `bitcoin-supported-transaction-core-integration.json` |

Root fixed a real Mode4 writer mismatch: its initial observation omitted the escrow-scope frame present in the first Before observation, leaving the referenced blob absent. Creation now uses the identical scope encoder. Strict reader checks, existing per-action observation bytes and non-inline initial bytes remain unchanged. Old affected histories stay refused. The failed gates and narrow correction are retained in `mode4-caller-start-core-integration.json`.

## Active workers

| Agent | Current isolated work | Boundary |
| --- | --- | --- |
| `row_continuation_impl` | Automatic Incoming/FileStepFrame to the actual native block tail, including nonempty financial rows and50ms retry/rejection interleavings | Direct borrower is integrated; automatic source evidence alone does not close this composition. |
| `v8_main` | Independently produced pending-plan/native-IO facts, new active WAL wiring and cold replay of recovery histories | Ordinary Mode4 replay is integrated; missing facts in older histories remain refusals. |
| `v8_clearlist` | Null Sync's actual detached initOrLoad child, source read/RPC release ordering and nonempty borrowed-header controls | Frozen candidate and genuine source extensions await Root Core gating; global startup remains open. |

## RC closure checklist

The independent static audit pins 58 read inputs and 23 source citation targets: `main-rc-gap-audit.json`, worker manifest `084726a002202b8e1622472baf600f2f257bc0a1103fa560bee7b62990258d6b`. It confirms row/catchup/recovery/publication and same-invocation consensus as implemented within their named domains. It adds no runtime coverage claim.

1. **Durable controls beyond Start.** Selected caller actions now pass their integrated WAL/recovery controls; broader admission remains explicit. Journal actual caller advance, delay, consensus, detached and timer actions with executed-once retention. Keep caller fulfillment separate from its child.
2. **Selected native recovery.** The selected controller passes actual Before/Intermediate/Post and second lost-return recovery controls. Bind exact origin, operation kind, job, method scope, ordinal, role and context. Reopen/supersede only that native operation, sync the mapping, then resolve Post once. Never choose the first pending operation implicitly.
3. **Remaining literal source branches.** Global/module initialization and notifications; uninitialized/no-chain Initialize and its 300 ms wait; The explicit primitive coercive Sync profile is tested; finish null SyncIndex and genuine detached jobs.
4. **File retry composition.** The direct retained borrower now passes five attempts/final50ms, partial snapshot/restore, literal errors and shutdown checks. Compose it with the active native service on the same Core/index/queue, including EBUSY/EPERM, ENOENT, partial file work, pause/height/shutdown interleavings and reorg behavior. Do not invent a financial rollback absent from the source.
5. **Mode4/caller cold source replay.** Ordinary Mode4 raw actions now reexecute through three actual native operations with full observations compared; finish caller/recovery domains. Rejected-append metadata and same-process recovery are tested; cold replay must still reexecute those raw rejected actions. New explicit producers may retain actual-stop IO facts or never-appended native-plan metadata before source observation; old Before/Accepted data must not supply those inputs.
6. **Fresh factual live input.** Start currently captures fixed RPC/documents facts. Add an explicit bounded recorded input action for fresh raw tip/block/transaction facts; preserve counters, active frames and financial state. The existing strict `source_main_step_update_io` is a useful primitive, not current active-service admission.
7. **Active-history continuation.** Rotate/publish acknowledged history past the actual 4,096-action limit while retaining the same owner and source/physical head. Native index paging alone does not remove this separate limit.

Then run the integrated RC path, consume collaborator fuzz results and perform the planned oracle-abuse fraud-proof exercise with UTXORef. Intended-host real-block CPU/RSS, directory/power-loss and actual killed-process financial recovery remain separate evidence gates. New standalone fee/reward matcher/payout wiring needs its named economic policy. The externally owned HashMap reducer candidate still needs representative host measurements.

## Resume details

- Repository: `C:/Users/patri/Documents/Codex/2026-10-26/go/outputs/TradeLayer-Rust`.
- Work area: `C:/Users/patri/Documents/Codex/2026-10-26/go/work`.
- Saved workbook: `outputs/completeness-6b9c6c5f/TradeLayer-completeness.xlsx` relative to the task root. Existing sidebar artifact: `artifact_g5Fs57Ux0BfP2n9VYK2wTTRQEDW2QMDU`.
- Exact current runner/log/provider pointers are in `work/root-resume-current.json`. Refresh the workbook/sidebar every 30 minutes when state changes, preserving layout and proof gates.
- Finished public-support provider: `work/providers/retained-callers-mode4-a94ea4c/libtradelayer_core.rlib`, SHA-256 `7f9b78d5169202225dee1aed5a03f8eeff6c189e6ea5ae98be062633c1de5560`; immutable provenance is adjacent. Candidate private Core tests still require Root execution.
- Root gates use `work/cargo-check.ps1 -CargoArguments @(...)`, the pinned 1.90 toolchain and one Cargo process. Debug target: `work/main-v12-target`; release target: `work/source-settlement-target`. Use the existing runners' temp and genuine numeric settings.
- Next action: gate the null Sync/automatic file and actual-fact milestones against current source, commit tested work and refresh this handoff. No pending worker result is complete merely because its files or helper tests exist.

## Latest rejected-append correction

Actual-source controls cover ordinary append, an intervening pause and genuine Before/Intermediate/Post recovery. They retain the full source observation, RPC/effect prefix, original financial/index owner and unresolved await after rejection. Metadata controls reject missing prior ACK, changed configuration/TLNM/observation, wrong error, an already resolved prior action and a false rejection masking a real extra native publication. A supported unmatched update checks the no-write case and one-time computed zero result; rejected ACKs never supply an inline closure. The original failed gates are retained. No new JavaScript oracle cases, transaction rows or authenticated proof claims are added.
