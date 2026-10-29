# TradeLayer-Rust live handoff

Updated: 2026-10-29T23:47:05Z. Refresh after every tested integration cycle.

## Current checkpoint

The tested implementation is `a94ea4c7cdf4677d7d48455d65363edd263a16c1`. Release evidence was committed as `4a85b00e8a5365e018e5b1f50fb347058e4af473`.

- **2,204 Rust executions:** 2,203 top-level tests plus one successful storage child, across 283 targets.
- **239 genuine BigNumber 9.1.2 checks:** the exact 87-target numeric CI list.
- Library: **549 passed, four ignored**. Doc tests completed with zero cases.
- All **1,888 tracked inputs** matched their frozen hashes before and after the run.
- Pinned Rust 1.90.0; release tests used offline/frozen dependencies. The latest code also passed all-target Clippy with warnings denied, Cargo formatting, formatting of 304 included Rust files, inventory and supply-policy checks.

Evidence: [release validation](fixtures/reference_fidelity/retained-callers-mode4-release-validation.json). Its adjacent input manifest preserves the frozen file list. The previous 2,142/239 release remains historical evidence.

The saved completeness workbook and existing sidebar agree on this release. Sixteen intentional cell edits preserved the 48 TL module rows, one external reference, all 37 transaction rows, formulas, styles, filters, panes and geometry. Zero formula errors. Evidence: `fixtures/reference_fidelity/retained-callers-mode4-release-completeness.json`.

**This is a passing development checkpoint, not a completed RC.** The source-composition and operational gates below remain open.

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
| `row_continuation_impl` | Selected caller/consensus/detached/timer actions, exact routed native selection and native-only recovery with durable mapping | Owns new selected-controller leaves and pure include/profile overlays. Does not own existing caller runtime/frame or the consensus factory. |
| `v8_main` | Ordinary concrete Mode4 raw-action/financial reexecution through actual original/copied historical views | Four new read-adapter metadata controls are a prerequisite, not financial replay. Old missing IO/plan facts remain explicit refusals. |
| `v8_clearlist` | Genuine JavaScript coercive SyncIndex characterization, then a narrow explicit caller profile | Owns existing caller runtime/frame branch changes plus new helper leaves. Preserve the old numeric profile and its visitor bytes. Null's detached initOrLoad path is broader and must not become synchronous success. |

The worker's first ten coercive Sync calls passed generation and check under genuine 9.3.1 and 9.1.2. This is source evidence only; the new Rust profile has not passed Root integration. Include the non-obvious primitive `"[object Object]" == {}` branch in characterization.

## RC closure checklist

The independent static audit pins 58 read inputs and 23 source citation targets: `main-rc-gap-audit.json`, worker manifest `084726a002202b8e1622472baf600f2f257bc0a1103fa560bee7b62990258d6b`. It confirms row/catchup/recovery/publication and same-invocation consensus as implemented within their named domains. It adds no runtime coverage claim.

1. **Durable controls beyond Start.** Journal actual caller advance, delay, consensus, detached and timer actions with executed-once retention. Keep caller fulfillment separate from its child.
2. **Selected native recovery.** Bind exact origin, operation kind, job, method scope, ordinal, role and context. Reopen/supersede only that native operation, sync the mapping, then resolve Post once. Never choose the first pending operation implicitly.
3. **Remaining literal source branches.** Global/module initialization and notifications; uninitialized/no-chain Initialize and its 300 ms wait; coercive/null SyncIndex branches and genuine detached jobs.
4. **File retry composition.** The separate file continuation already models five attempts and the final 50 ms wait. Compose it with the active native service on the same Core/index/queue, including EBUSY/EPERM, ENOENT, partial file work, pause/height/shutdown interleavings and reorg behavior. Do not invent a financial rollback absent from the source.
5. **Mode4/caller cold source replay.** Reexecute raw actions against verified original/copied metadata and compare all observations. Characterize repeated AppendPending API rejection. New explicit producers may retain actual-stop IO facts or never-appended native-plan metadata before source observation; old Before/Accepted data must not supply those inputs.
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
- Next action: validate the first frozen worker milestone against current source, record exact checks, commit it and refresh this handoff. No pending worker result is complete merely because its files or helper tests exist.
