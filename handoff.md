# TradeLayer-Rust live handoff

Updated: 2026-10-30T03:59:52Z. Refresh after every tested integration cycle.

## Current checkpoint

The last complete-release implementation is `a94ea4c7cdf4677d7d48455d65363edd263a16c1`. Release evidence was committed as `4a85b00e8a5365e018e5b1f50fb347058e4af473`.

- **2,204 Rust executions:** 2,203 top-level tests plus one successful storage child, across 283 targets.
- **239 genuine BigNumber 9.1.2 checks:** the exact 87-target numeric CI list.
- Library: **549 passed, four ignored**. Doc tests completed with zero cases.
- All **1,888 tracked inputs** matched their frozen hashes before and after the run.
- Pinned Rust 1.90.0; release tests used offline/frozen dependencies. That release also passed all-target Clippy with warnings denied, Cargo formatting, formatting of 304 included Rust files, inventory and supply-policy checks.

Evidence: [release validation](fixtures/reference_fidelity/retained-callers-mode4-release-validation.json). Its adjacent input manifest preserves the frozen file list. The previous 2,142/239 release remains historical evidence.

The saved completeness workbook and existing sidebar agree on this release. Sixteen intentional cell edits preserved the 48 TL module rows, one external reference, all 37 transaction rows, formulas, styles, filters, panes and geometry. Zero formula errors. Evidence: `fixtures/reference_fidelity/retained-callers-mode4-release-completeness.json`.

Later focused correction: rejected append ACKs now preserve the prior original/replacement native relation, while a current API rejection cannot reuse an older StorageStopped result. Five new actual source and three metadata controls pass; library **557/four ignored**, 44 public checks, Clippy/fmt304/inventory/supply pass. Evidence: `mode4-rejected-append-core-integration.json`. The full release remains 2,204/239 until the next complete run.

**This is a passing development checkpoint, not a completed RC.** The source-composition and operational gates below remain open.

## Latest tested orchestration

The latest batch passes **638 library tests (four ignored), 44 public checks, all-target Clippy with warnings denied, Cargo formatting, 372 included-file formatting checks, inventory and supply policy**. All 44 new unit tests pass. Counts overlap with earlier/full runs; do not add focused runs to the full release total. Evidence: `fixtures/reference_fidelity/main-orchestration-core-integration.json`.

- Null Sync preserves separate parent/detached source fulfillment and the same actual financial header/index/queue. A header cannot close while its borrowed block or unadopted completion remains. It closes once at the safe boundary while a future timer may remain pending.
- Automatic Incoming/file/native execution preserves partial work, all five retries and the final50ms wait, same native plan through distinct Append/Resolve, shutdown and literal physical restore without an invented balance rollback. Its new automatic-only parked tail allows an independent file retry while financial/native work remains held. Actual trace ranges distinguish already-owned effects from direct Kernel events; no value deduplication is used. Legacy/default queue rules and visitor bytes remain unchanged.
- Explicit13-2 actual-fact journaling syncs the real pending plan/selector and typed native IO result before source observation or Accepted/ACK. Native-only Before/Intermediate/Post recovery, real mapped replacement/escrow and a second applied/lost return retain one actual owner. Every IO result remains in an ordered chain; verified Post is not reappended and no source prefix/constructor/decoder is repeated.

Source checks preserve the original JavaScript: null first-ten10calls/36records, held-live1call/3records, four automatic scenes/21records and the parallel two-timer8record scene under both genuine numeric profiles. The old probe's NotQuiescent refusal is retained; extending the unreleased automatic domain was explicit. Empty processBlockData legitimately makes no native write. Failed gates, wrong test filter/corpus selection and precise corrections are recorded; zero-test runs count as no coverage.

This remains a development checkpoint. Full release evidence is still **2,204 Rust /239 numeric** at `a94ea4c7`. The complete13-2 cold source actor, fresh feed, raw/global startup, history continuation and authenticated proof gates remain open.

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
| `row_continuation_impl` | Typed fresh future RPC/document feed, source10/44 plus service-chain evidence; frozen1064 Core controls await Root | No old action/WAL/cold admission and no invented fine-grained RPC suspension. |
| `v8_main` | Complete checked13-2 physical trace/fact reader and one-owner raw-source cold actor | Actual-fact writer/native recovery are integrated; no Before/Accepted reconstruction or implicit old-reader admission. |
| `v8_clearlist` | Explicit unconfigured raw startup foundation, literal shared DB promises/retry and Initialize suffix | f495/a70 prefix proposals and genuine source evidence remain outside Root until Core integration. Configured owners are never reset. |

## RC closure checklist

1. Preserve the already-tested selected caller WAL/recovery and same-invocation consensus. New startup/feed/file profiles need their own explicit durable admission.
2. Finish complete13-2 physical trace/ACK/escrow validation and raw-source/financial cold replay using independently produced facts. Old missing IO/plan facts remain refusals; Before/Accepted are comparisons.
3. Integrate the raw/global startup and actual Initialize suffix, including original module versus internal shared DB promise identity, legitimate DB retries, constructor/static init and500/300/1500 waits. Null/primitive Sync cover their named tested domains; further startup is not synchronous success.
4. Complete automatic-file native ambiguity, durable action history and cold playback. The same-process timed file/native/parallel source path is tested; no cross-profile recovery admission is implied.
5. Gate the typed fresh factual tip/block/transaction/document feed, preserving current counters, actual captured awaits and the same financial/index/source owner. Frozen1064 remains a candidate.
6. Rotate/publish acknowledged active-service history beyond the separate4096-action limit while retaining source/physical heads and one owner. Native paging alone does not close this.
7. Run integrated RC checks and consume collaborator fuzz findings, then the planned oracle-abuse exercise with UTXORef as a separate consumer.

Intended-host real-block CPU/RSS, directory/power-loss, actual killed-process financial recovery, authenticated input/actor/UTXO context and complete-block proofs remain separate evidence gates. Standalone fee/reward policy wiring remains explicit. The externally owned HashMap promotion candidate still needs representative intended-host measurements. No hash establishes authentication.

## Resume details

- Repository: `C:/Users/patri/Documents/Codex/2026-10-26/go/outputs/TradeLayer-Rust`.
- Work area: `C:/Users/patri/Documents/Codex/2026-10-26/go/work`.
- Saved workbook: `outputs/completeness-6b9c6c5f/TradeLayer-completeness.xlsx` relative to the task root. Existing sidebar artifact: `artifact_g5Fs57Ux0BfP2n9VYK2wTTRQEDW2QMDU`.
- Exact current runner/log/provider pointers are in `work/root-resume-current.json`. Refresh the workbook/sidebar every 30 minutes when state changes, preserving layout and proof gates.
- Finished public-support provider: `work/providers/retained-callers-mode4-a94ea4c/libtradelayer_core.rlib`, SHA-256 `7f9b78d5169202225dee1aed5a03f8eeff6c189e6ea5ae98be062633c1de5560`; immutable provenance is adjacent. Candidate private Core tests still require Root execution.
- Root gates use `work/cargo-check.ps1 -CargoArguments @(...)`, the pinned 1.90 toolchain and one Cargo process. Debug target: `work/main-v12-target`; release target: `work/source-settlement-target`. Use the existing runners' temp and genuine numeric settings.
- Next action: gate frozen fresh-input/startup and complete source-fact cold replay milestones against current source, commit tested work and refresh this handoff. No pending worker result is complete merely because its files or helper tests exist.

## Latest rejected-append correction

Actual-source controls cover ordinary append, an intervening pause and genuine Before/Intermediate/Post recovery. They retain the full source observation, RPC/effect prefix, original financial/index owner and unresolved await after rejection. Metadata controls reject missing prior ACK, changed configuration/TLNM/observation, wrong error, an already resolved prior action and a false rejection masking a real extra native publication. A supported unmatched update checks the no-write case and one-time computed zero result; rejected ACKs never supply an inline closure. The original failed gates are retained. No new JavaScript oracle cases, transaction rows or authenticated proof claims are added.
