# TradeLayer-Rust live handoff

Updated: 2026-10-30T08:31:26Z. Refresh this file after every tested integration or PR cycle.

## Latest local checkpoint

The current branch contains the tested pin/fresh/cold/startup batch described in `fixtures/reference_fidelity/main-pin-fresh-cold-core-integration.json`. Validation: **715 library tests passed, four ignored, 44 public compatibility checks, 32 new controls, strict all-target Clippy, Cargo formatting, 441 included Rust files, inventory and supply policy**. This is one complete715-case library run. Failed and interrupted attempts are preserved in the record and do not count as passed suites. No push, publication, PR or merge was requested or performed.

The last full all-target release remains implementation `a94ea4c7cdf4677d7d48455d65363edd263a16c1`, evidence `4a85b00e8a5365e018e5b1f50fb347058e4af473`: **2,204 Rust executions** (2,203 top-level plus one storage child) across283targets, **239 genuine BigNumber9.1.2 checks** from the exact87-target numeric list. All1,888 frozen inputs matched. Evidence: `fixtures/reference_fidelity/retained-callers-mode4-release-validation.json`. Do not add focused counts to that baseline or treat the latest library run as an all-target release. **This is a development checkpoint; the RC is unfinished.**

## What this batch establishes

- WAL preparation reuses pins computed from the same immutable new blobs within that call. Three comparisons preserve exact old-body output bytes, error precedence, existing-file read order and sync barriers. No cross-call cache, benchmark speedup or peak-RSS claim is added.
- The explicit durable fresh-input coordinator syncs an outer raw Intent before publishing a factual feed or running a source action/constructor. Seven actual-owner controls retain the same financial runtime, native index, held plan, route and feed history through ambiguous control writes and native Before/Post recovery. Observation/control retry never repeats source or feed. Paired fresh-input cold replay is still separate.
- The ordinary segmented cold actor reexecutes4,098 raw actions across16sealedsegments on one financial/index owner. Complete source observations, financial state, Kernel, receipts and physical heads are comparisons. Nonempty checkpoints around Append/Resolve pass with zero native append/copy/compaction IO during cold execution. Ordinary ambiguity without independent typed native IO facts remains refused.
- Typed literal startup now observes the actual equal-tip branch and executes its inner consensus call followed by outer Initialize's second call. Seventeen raw-startup plus two Kernel controls pass, including the original two incoming timers, startup clock, same header/queue, real financial/native await, selected deterministic JS financial tables and owner-bound descriptors. Generic/global startup, complete random-ID/effect replay and all concurrent block slots are not established.

Two checks exposed real boundaries. The fresh test attempted a blob read through a correctly ambiguous journal: it now asserts that refusal, resolves sealed Post, rechecks unchanged finance/no constructor, and compares the persisted archive before advancing source. The segmented producer pinned a BASE origin observation but persisted only its extended first Before; it now supplies the exact actual BASE bytes/ref through the existing first Intent barrier, before source execution. The immutable pin is checked; readers and persisted schemas are unchanged. Old histories missing that blob remain refused. The all-target check also exposed a missing genuine prefix module in the copied-Kernel canonical test driver. That driver now imports the actual production source for its private clock type; all13 checks pass. Production Rust and test assertions are unchanged by this driver correction.

## Mandatory next logic

1. Complete the paired fresh-feed cold interleave: independently retained outer raw Feed links must justify every inner observation gap. Old public13-2 readers remain strict. No expected financial state/result is an execution input.
2. Add independently produced ordinary segmented pending-plan/native-IO facts and recovery/cold composition. Preserve the complete owner and original actual result if post-IO serialization fails. Never retry the source prefix or financial action.
3. Close genuine first-live-await startup concurrency. New genuine source evidence shows outer Initialize may fulfill while a discarded native-row await remains held; outer writes can advance the index in that interval. The required late operation is the original captured metadata query/update only, with sealed actual commit-result resolution after later commits. Do not rerun finance, decoders or failure hooks, nor weaken old prepared-plan/current-Post contracts. Simultaneous live block capacity, early pending DB factory and exact broader Promise ordering remain explicit gates.
4. Finish genuine NeDB entropy/ID/full-effect replay and raw/global initializer composition. Actual crypto draws must pass unchanged into NeDB. A prior synthetic RNG proposal was rejected and was not applied; do not normalize IDs or derive inputs from expected outputs.
5. Complete automatic-file native ambiguity/durable cold composition, actual killed-process financial recovery, intended-host directory/power-loss and representative real-block CPU/RSS measurements. The externally owned HashMap promotion candidate still needs host evidence. Standalone fee/reward policy wiring stays explicit.
6. Run fresh integrated RC/all-target/numeric checks, consume collaborator fuzz findings, then perform the planned mock oracle-abuse fraud-proof exercise. TL supplies checkable oracle state; **UTXORef is a separate consumer repository**, and optional Binohash belongs there. Authentication and complete-block proofs remain separate open gates.

## Workers and ownership

| Worker | Isolated next work |
| --- | --- |
| `row_continuation_impl` | Independently typed ordinary native IO facts and segmented cold recovery. |
| `v8_main` | Paired durable fresh-feed cold replay with actual outer facts and strict older readers. |
| `v8_clearlist` | Genuine held-first-await/outer concurrency, late metadata operation/results, early DB factory and full entropy replay. |

The external web worker is independent; unavailable handoff text or unexecuted source code is not integrated work. Root owns repository writes and commits. Apply immutable worker manifests through pure transforms of CURRENT source; never overlay stale whole files. Run one whole-Core Cargo process at a time. Agents use isolated stages and must not run Root/private Core.

Reference JS is pinned at `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Preserve original bugs, throws, ordering and Promise lifetimes. Expected Before/Accepted, state, results, validity verdicts and receipts are comparisons only. Uncertain IO retains the one real financial/index/source/queue owner; reconcile only the exact native/control operation. A hash is not authentication. Do not infer whole-module parity from inventories or selected profiles.

Commit completed checked work on this branch; leave unrelated changes untouched. Never push, publish, open a PR or merge without explicit instruction. Before an authorized Rust push, run `bash scripts/ci-preflight.sh --full`; GitHub Lean/Kani/audit gates remain separate.

## Resume and tracker

- Repository: `C:/Users/patri/Documents/Codex/2026-10-26/go/outputs/TradeLayer-Rust`.
- Work/runners/logs: `C:/Users/patri/Documents/Codex/2026-10-26/go/work`; exact session state is in `work/root-resume-current.json`.
- Frozen current build inputs: `work/root-pin-fresh-cold-final-inputs.json`; runner `work/check-pin-fresh-cold-final.ps1`. Finished scripts/mounters are one-time operations. Do not restart completed long checks merely for a status update.
- Root gate wrapper: `work/cargo-check.ps1`, pinned Rust1.90/MSVC; debug target `work/main-v12-target`, release target `work/source-settlement-target`. Reuse the recorded task TMP and genuine numeric settings. Separate scratch directories for concurrent direct test binaries; only one Core Cargo.
- Immutable public provider: `work/providers/retained-callers-mode4-a94ea4c/libtradelayer_core.rlib`, SHA256 `7f9b78d5169202225dee1aed5a03f8eeff6c189e6ea5ae98be062633c1de5560`, provenance adjacent. It is older public support, not new private-Core validation.
- Saved workbook: `outputs/completeness-6b9c6c5f/TradeLayer-completeness.xlsx` relative to task root. Existing sidebar: `artifact_g5Fs57Ux0BfP2n9VYK2wTTRQEDW2QMDU`. Keep both consistent and refresh after meaningful verified changes, approximately every30minutes. Preserve48TL module rows, one external reference,37transaction rows, formulas/styles/filters/panes/geometry and all remaining gates. The previous verified workbook checkpoint was9f901cb with683 distinct library tests/44public; update only after this code commit.
