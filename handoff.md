# TradeLayer-Rust live handoff

Updated: 2026-10-30T06:11:45Z. Refresh after every tested integration cycle.

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

## Latest tested Initialize/cold/history work

The latest tested code is `9f901cb473313f8797ba4becc8bf550a9f5f9b08`. This batch covers **683 distinct library tests (four ignored), 44 public checks, strict all-target Clippy, Cargo formatting, 411 included-file formatting checks, inventory and supply policy**. Twenty-five new controls pass. The exact passed-name audit joins680 ordinary regression cases with the two long cases and corrected Initialize case; production Rust is identical across those runs. It does not claim one683-case run. Evidence: `fixtures/reference_fidelity/main-initialize-cold-core-integration.json`. Full release remains **2,204 Rust /239 numeric** at `a94ea4c7`; this is not a completed RC.

- The actual raw startup owner/header moves into the existing Initialize/consensus body without constructor or1500/500/300 replay. Physical insert and source await resolution stay distinct. Three suffix controls plus eight inherited prefix controls pass, including warm null/undefined getTest errors. This establishes named empty caller fulfillment. Source inspection found a remaining real omission: equal-tip sync returns a nested constructOrLoadConsensus, then outer Initialize calls it again. Exact two-job/timer/full-effect parity remains open and must not be masked by the present tests.
- The finite13-2 actual-fact cold actor reconstructs ONE financial owner using the same checked-prefix and physical verification bodies. Five real writer histories compare complete financial state, effects, Kernel, receipts and source Before/Accepted. Actual Before replacement, second lost return/Post and genuine reappend rejection pass without source/financial retry and with zero native append/copy/compaction IO. Recorded environmental facts are conditional inputs; observed state/results remain comparisons.
- The new ordinary-HISTORY segmented live owner passes4,097 actual Pause calls plus Start:4,098 total actions over16sealedsegments, full raw action/entry/physical-ACK head chain retained. Same owner/plan survives checkpoints before/after append and actual nativePost reopen. Receipt RAM leaves only after a same-owner checkpoint proof. Nine metadata controls separately cover4,352ACKs/17segments and real failures. The source-count wall is removed in this explicit profile; disk/effects/Kernel bounds and cold/fresh/caller/file/13-2 composition are still separate.

JavaScript17calls/92records reproduce under both genuine numeric profiles. Genuine nonempty source runs additionally expose NeDB random IDs; their captured crypto draws and complete outputs remain outside this Root batch. A proposed synthetic RNG hook was rejected by automatic review and was not applied. Transparent observation forwards genuine crypto unchanged. Full entropy replay/table equality and nested/nonempty/global startup are unfinished.

## Latest tested startup and trace prerequisite

The latest tested code is `79a30c6f4ec0ec321ad0968123cab910f77a4978`. This batch passes **658 library tests (four ignored), 44 public checks, strict all-target Clippy, Cargo formatting, 392 included-file formatting checks, inventory and supply policy**. Ten new actual-Core controls pass: eight startup and two independently produced writer-trace controls. Evidence: `fixtures/reference_fidelity/main-startup-trace-core-integration.json`. Full release evidence stays **2,204 Rust /239 numeric** at `a94ea4c7`; this is not a completed RC.

The typed raw startup owner consumes an unconfigured actual Replay and the same native index. TxUtils import, DB automatic initialization and Main constructor/static init run in source order. The prefix keeps the original module promise distinct from the shared internal promise, preserves1500/500/300 waits and genuine partial failures, and performs the legitimate new DB attempt after an earlier automatic rejection. SDK own fields are distinct from RPC getChain/getTest facts. Configured owners are never reset, stale inputs refuse unchanged, and old Invoke/witness namespaces remain closed. Eight controls establish named DB/throw prefix projections; they do not establish complete effects or Rust Initialize suffix fulfillment.

The separate13-2 trace reader checks mandatory facts, actual producer WAL prefixes and ordered native IO/ACK barriers. Old HISTORY/Mode4 profiles retain their own constructors and proof brands. The physical verification body and completed-prefix reconstruction body are shared literally; their old paths pass regression checks. Positive13-2 native-page and completed-prefix composition still needs the full actor controls. Real three-plan writer history passes; a corrupt mandatory fact and changed retained trace refuse without repair. No decoded pin becomes a live native permit. The full raw-source financial cold actor remains unfinished.

The unchanged JavaScript reproduces15 calls/86 records under both genuine BigNumber profiles. Some source cases execute the complete empty Initialize; those source-only observations are not Rust suffix coverage. Record counts include inputs. Full/global startup, nonempty startup-to-live finance, financial cold replay and active-history continuation remain open.

## Latest tested fresh input

The latest tested code is `5ae93f188110ba1dd36088e2280244789ad7e053`. This batch passes **648 library tests (four ignored), 44 public checks, all-target Clippy with warnings denied, Cargo formatting, 378 included-file formatting checks, inventory and supply policy**. Ten new controls cover the new canonical raw-fact codec and actual financial owners. Evidence: `fixtures/reference_fidelity/main-fresh-input-core-integration.json`. The complete release baseline remains **2,204 Rust /239 numeric** at `a94ea4c7`; do not add overlapping focused counts or promote this to a completed RC.

The typed fresh-input service/caller consumes the existing owner. It changes only future tip/block/network/transaction-document facts, preserving counters, client, captured source locals, actual prepared native plan, physical index, route, pending timer and borrowed financial queue. Retained caller and service future document sets change together. Invalid, observation-pending and terminal feeds refuse before publication. Six codec and four actual-Core controls pass. Original JavaScript reproduces ten cases/44 records plus one service-chain case/four records under genuine BigNumber 9.3.1 and 9.1.2. Record counts include raw inputs.

This is in-memory factual admission. Durable feed actions/cold replay, native ambiguity through this wrapper and fine-grained raw-RPC Promise suspension remain open. Existing old codecs/readers/default bodies are unchanged; no authentication or new state-import authority is established.

## Latest tested orchestration

The prior orchestration checkpoint is `b8546b3fa7624b22d2b3853951104198429ce77f`. This batch passes **638 library tests (four ignored), 44 public checks, all-target Clippy with warnings denied, Cargo formatting, 372 included-file formatting checks, inventory and supply policy**. All 44 new unit tests pass. Counts overlap with earlier/full runs; do not add focused runs to the full release total. Evidence: `fixtures/reference_fidelity/main-orchestration-core-integration.json`.

- Null Sync preserves separate parent/detached source fulfillment and the same actual financial header/index/queue. A header cannot close while its borrowed block or unadopted completion remains. It closes once at the safe boundary while a future timer may remain pending.
- Automatic Incoming/file/native execution preserves partial work, all five retries and the final50ms wait, same native plan through distinct Append/Resolve, shutdown and literal physical restore without an invented balance rollback. Its new automatic-only parked tail allows an independent file retry while financial/native work remains held. Actual trace ranges distinguish already-owned effects from direct Kernel events; no value deduplication is used. Legacy/default queue rules and visitor bytes remain unchanged.
- Explicit13-2 actual-fact journaling syncs the real pending plan/selector and typed native IO result before source observation or Accepted/ACK. Native-only Before/Intermediate/Post recovery, real mapped replacement/escrow and a second applied/lost return retain one actual owner. Every IO result remains in an ordered chain; verified Post is not reappended and no source prefix/constructor/decoder is repeated.

Source checks preserve the original JavaScript: null first-ten10calls/36records, held-live1call/3records, four automatic scenes/21records and the parallel two-timer8record scene under both genuine numeric profiles. The old probe's NotQuiescent refusal is retained; extending the unreleased automatic domain was explicit. Empty processBlockData legitimately makes no native write. Failed gates, wrong test filter/corpus selection and precise corrections are recorded; zero-test runs count as no coverage.

This remains a development checkpoint. Full release evidence is still **2,204 Rust /239 numeric** at `a94ea4c7`. The complete13-2 financial cold source actor, durable fresh-feed composition, raw/global startup, history continuation and authenticated proof gates remain open.

## Rules and ownership

Reference JavaScript: `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Preserve literal source behavior, including named source bugs and throws. A repaired protocol needs an explicit policy; do not silently substitute successful behavior.

Keep one actual financial owner, native index and borrowed queue. Expected financial tables, results, validity verdicts, Before/Accepted observations and receipts are comparisons, never execution inputs. Uncertain IO must retain that owner and retry only the exact native/control operation; never repeat the source constructor, Main action, decoder or financial prefix.

Root owns repository edits, checks and commits. Run only one whole-Core Cargo process at a time. Agents use isolated stages and return immutable manifests; apply pure transforms to current files rather than stale whole-file overlays. Commit tested work on the current branch. Never push, publish, open a PR or merge without an explicit request. Preserve unrelated changes. Before any authorized Rust push, run the full preflight required by `AGENTS.md`; Lean/Kani/GitHub-only checks are separate.

TL produces checkable state-oracle data. **UTXORef is a separate consumer repository.** Optional Binohash work belongs there. Hashes, copied history and claimed actors do not establish authentication.

## Integrated scope

Record names in the table are under `fixtures/reference_fidelity/`. The frozen complete-release manifest defines that release; later focused records are not implicitly included. Counts overlap; never add focused runs to the release total.

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
| `row_continuation_impl` | Fresh input integrated; extending acknowledged active history beyond 4096 on the same owner | No old action/WAL/cold admission and no invented fine-grained RPC suspension. |
| `v8_main` | Strict13-2 trace/checked-prefix integrated; completing the one-owner raw-source financial cold actor | Actual-fact writer/native recovery are integrated; no Before/Accepted reconstruction or implicit old-reader admission. |
| `v8_clearlist` | Explicit unconfigured raw startup foundation, literal shared DB promises/retry and Initialize suffix | f495/a70/SDK06d9 prefix is Root-tested; actual Initialize suffix remains isolated. Configured owners are never reset. |

## RC closure checklist

1. Preserve the already-tested selected caller WAL/recovery and same-invocation consensus. New startup/feed/file profiles need their own explicit durable admission.
2. Finish complete13-2 physical trace/ACK/escrow validation and raw-source/financial cold replay using independently produced facts. Old missing IO/plan facts remain refusals; Before/Accepted are comparisons.
3. Integrate the raw/global startup and actual Initialize suffix, including original module versus internal shared DB promise identity, legitimate DB retries, constructor/static init and500/300/1500 waits. Null/primitive Sync cover their named tested domains; further startup is not synchronous success.
4. Complete automatic-file native ambiguity, durable action history and cold playback. The same-process timed file/native/parallel source path is tested; no cross-profile recovery admission is implied.
5. Complete durable fresh-feed actions/cold composition and fine-grained raw-RPC continuation. The same-owner in-memory factual feed is tested; it preserves current counters, captured awaits and financial/index/source ownership.
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
- Next action: gate the queued WAL pin-reuse optimization, durable fresh coordinator/cold interleave, nonempty/nested startup and segmented cold/native-fact milestones; then run integrated RC/fuzz checks. Preserve exact old profiles and one actual owner. No pending worker result is complete merely because its files or helper tests exist.

## Latest rejected-append correction

Actual-source controls cover ordinary append, an intervening pause and genuine Before/Intermediate/Post recovery. They retain the full source observation, RPC/effect prefix, original financial/index owner and unresolved await after rejection. Metadata controls reject missing prior ACK, changed configuration/TLNM/observation, wrong error, an already resolved prior action and a false rejection masking a real extra native publication. A supported unmatched update checks the no-write case and one-time computed zero result; rejected ACKs never supply an inline closure. The original failed gates are retained. No new JavaScript oracle cases, transaction rows or authenticated proof claims are added.

## Latest scoped workbook refresh

Saved/sidebar agree on `9f901cb`: 683 distinct library tests /44 public,25newcontrols, exact passed-name audit and unchanged production Rust across the combined runs. Full release remains2,204 Rust /239 numeric at a94ea4c7. Seventeen intentional edits preserve48TL modules, one external reference,37transaction rows, formulas/styles/filters/panes/geometry and all proof/authentication gates. Zero formula errors. Evidence:`fixtures/reference_fidelity/main-initialize-cold-completeness.json`. Nested/nonempty/global startup and durable fresh/file/segmented cold composition remain open.
