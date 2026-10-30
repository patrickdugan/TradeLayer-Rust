# Claude handoff — conserve Codex

Prepared 2026-10-30 UTC at the user's request. Continue the full TradeLayer-Rust port; it is **not an RC or a completed faithful port**. Codex workers have stopped, and the periodic Codex spreadsheet refresh is paused. No push, PR or merge was performed.

## Open this checkout

```text
Repository: C:\Users\patri\Documents\Codex\2026-10-26\go\outputs\TradeLayer-Rust
Work/evidence: C:\Users\patri\Documents\Codex\2026-10-26\go\work
Pinned JS: C:\Users\patri\Documents\Codex\tradelayer.js
Pinned JS commit: dca0888b827c5bc28fa9189bc9e1d507f5c5670c
Original roadmap: C:\Users\patri\.codex\attachments\d5ec0de3-0050-4aab-8ba7-75913036b2e1\pasted-text-1.txt
```

These are real files on the shared Windows filesystem. No download from another worker's container is required. Use the current checkout, not an older copied source tree. All `work/` paths below are relative to the `go` folder, not the repository.

Read this file, `handoff.md`, `docs/COMPLETENESS_STATUS.json`, the roadmap and the three worker handoffs below. Inspect Git and the current files before relying on a historical statement. Keep this handoff current after each tested integration cycle.

Claude is now the integration owner. Historical worker statements such as "no Root edits/Core/commit" describe their former delegated lanes; they do not forbid Claude's authorized integration and checked commits. Committed copies of their final notes are also in `docs/claude-handoff/`.

## User rules and architectural constraints

- Commit completed, checked local work on the current branch. Stage only task-owned files; preserve unrelated changes. Never push, publish, merge or open a PR without explicit user authorization.
- Preserve pinned JS behavior, including throws, ordering, partial writes and source bugs. Keep explicit repaired policies separate. Unsupported domains and bounded component checks do not establish full parity.
- One actual financial Ledger/Core/index/queue owner. Never execute from expected state, recorded Before/Accepted, a supplied verdict, or an observed scheduling trace. Actual IO results must remain owned through observation/callback failures. Native retry must not repeat the financial/source prefix or fabricate a Post.
- Preserve old/default APIs, codecs, readers and admission unless a change is explicitly justified and checked. Apply pure transforms to **current** text; do not replace current files with old full-file previews. Frozen packages are immutable.
- TL produces checkable state-oracle data. UTXORef is a **separate consumer repository**; optional Binohash belongs there. A hash is not authentication.
- There are 48 TL module rows, one external reference and 37 transaction audit rows. Inventory counts are not completion percentages.
- Use one whole-Core Cargo process at a time. A timeout or lock file is not proof that a process stopped. All Codex-owned processes listed here have actually finished; no worker process remains active.

## Committed and verified

Pre-handoff checkpoint was `186054725b4e957352cd38f5a0b9af525e12716d`. The handoff itself adds a later documentation commit; use `git log` for its SHA.

| Commit | Result |
|---|---|
| `6a62e3851054267da72516fe4f6f284a7bba84cb` | Three literal orderbook lifecycle helpers on the existing Queue/Core/registry. Five new tests and eight existing queue tests pass. |
| `273511c4ad12c18be28c55a8e83f7ffbd643ef3e` | Pins the two source fixtures and generator to LF. A clean committed export now passes all 13 public tests and all four genuine source full/proof10 regenerations (BigNumber 9.3.1 and 9.1.2). Do not weaken the byte-exact proof assertion. |
| `33a1b49b5ecdc635c1496710c70f4e11c21d1738` | Mixed ordinary/inline cold source replay: eight actual financial-owner and four metadata checks pass, also all 12 from its clean committed export. Before retry, original/second lost-return Post, API rejection and the unwritten-await negative preserve real financial/effect/source state. |
| `1860547` | Records the committed checks and synchronized completeness workbook/sidebar (version 195). |

The last full all-target release evidence remains **2,204 Rust executions / 239 genuine locked-reference checks**, at code `a94ea4c`. Later **782 scoped library passes** are a separate, overlapping historical run with four ignored tests and two known failed uncommitted queued positives excluded. Do not add these counts or promote a new full-release total.

Evidence: `fixtures/reference_fidelity/orderbook-lifecycle-helpers-core-integration.json`, `main-mixed-native-cold-core-integration.json`, `mixed-cold-orderbook-completeness.json`, and the prior `main-mixed-native-broad-library-validation.json`.

The cold negative's decode accessor reads a **thread-local total**, not a per-Runtime counter. It correctly measures independent live/cold deltas of one without resetting or suppressing calls. Its other strong state/effects/action-count/PC/error/no-Post assertions remain intact.

## Committed by Claude (2026-10-31)

The 18-check outer/query/intermediate increment is committed with the dd36 visibility fix (`take_prepared` private). It was validated in a clean export of exactly the committed blobs (`work/tlx`, runner `work/claude-check-outer-increment.ps1`, logs `work/claude-outer-increment-r3-*.log`): 12 outer/query + 5 native fixed-read + 9 mixed cold owners pass; strict all-target Clippy, Cargo fmt, 519 included files, inventory and supply pass, and the full library on the same export passes (806 passed, 4 ignored, 0 failed; library only, not all targets). Evidence: `fixtures/reference_fidelity/main-outer-bootstrap-intermediate-core-integration.json`.

A pre-existing defect was fixed in the same commit: the `LOGIC_BY_TX_TYPE.json` pin for `fixtures/legacy_types_source_types.txt` hashed a CRLF autocrlf checkout, so the inventory gate failed on any clean (Linux CI) checkout. It now pins the committed LF blob (`a49c1b1f`, the hash every other consumer already uses) with `lfNormalized`.

The failed queued-startup proposal is still only in the dirty checkout and was not staged: the three Runtime includes, the six one-line include edits, the queued leaves, their CI/preflight lines and pins. The working-tree `LOGIC_BY_TX_TYPE.json`, `ci.yml`, `ci-preflight.sh` and `source_contract_runtime.rs` therefore still differ from HEAD by exactly that proposal.

Tooling notes: this machine has only Windows PowerShell 5.1, which turns native stderr into terminating errors under `$ErrorActionPreference=Stop`; run gates through `cmd /c ... > log 2>&1`. `cargo fmt --all` fails with os error 206 (command line too long) when the checkout path is longer than the repository path, so exports go to a short path.

### First next action

Implement the strict native raw-request capture transform (step 2 of `work/native-main-outer-bootstrap-kernel-binding-v13/CLAUDE_HANDOFF.md`) in a new revision against current one-line formatting, then bind outer hooks to the same `MainStepKernelV1::startup_source_parts()` queue.

## Main remaining blocker: actual startup scheduling

Two uncommitted queued positives still fail: the proposed Rust scheduler samples an outer fixed index view **after** the first live upsert, yielding four rows; genuine pinned JS samples three rows **before** the first transaction RPC/upsert. Later live writes are therefore absent. The old assertions have not been weakened or ignored in CI.

The correction requires the actual same Kernel to own intrinsic Promise adoption, caller-return subscriptions, Incoming/Sync/checkBlockLag, named NeDB request/executor/candidate/callback jobs and the real nonclone financial/index/header/query frames. Modeling NeDB's observable scheduling is reference compatibility; durable production storage remains the compact-index architecture.

No recorded delay, phase count, operation/effect offset, expected result, financial snapshot or extra owner may drive execution. Request-time hooks, row sampling, callback delivery and source reactions are distinct. Register parent awaits at actual async-call return, not only at eventual child completion. Retain the independent unawaited Tally child. The actual outer prefix/prepared plan is not completed Main execution.

Read these **local** detailed handoffs:

1. `work/native-main-outer-bootstrap-kernel-binding-v13/CLAUDE_HANDOFF.md`: exact hooks/registry/API seams. Hook leaves are parse-only WIP; the native raw-request transform fails a strict multiline-vs-one-line visitor anchor. No completed outer Kernel registry exists.
2. `work/native-main-startup-source-kernel-v13/CLAUDE_CHECKPOINT.md`: same-Kernel Promise/NeDB components and the still-unimplemented Incoming/return-chain controller.
3. `work/review-main-startup-source-kernel-v13/root-ready-review-1/REVIEW.md`: independently reproduced notification-order defects and custody review.

Useful frozen manifests (resolve under `work/`; verify full payload hashes):

| Package | Manifest SHA256 |
|---|---|
| `native-main-startup-query-await-v13/root-ready-source-query-await-1` | `54a94229f992c45b9d5e703f536f373e955bada6b3c6e6e7f0af022f4eaa5da2` |
| `native-main-outer-bootstrap-await-v13/root-ready-outer-bootstrap-await-3` | `25573cb771b6a993d807984b383e9e5cf9c266830857f6f537c7141b69edd16e` |
| Outer `root-ready-per-factory-stop-1` | `dd7b992235187346d4c71fed0ed178f54cb089e94e0ea6fe52c3956b5dc3d28e` |
| Outer `root-ready-native-return-conversion-1` | `c0e83a76c7f9f978deb789d219f9d6023a4d3d3a3aad81448603c8d8177bbc4b` |
| `native-main-mixed-cold-intermediate-v13/root-ready-mixed-cold-intermediate-1` | `63f688d4f3e6efeb1fc405f7afddd8a9ff91aa8161a90edbb1349a5d334eb3f8` |
| `native-main-startup-source-kernel-v13/root-ready-source-kernel-1` | `07687ab53d680888194ac96fee638112b503bab1ca663aa440e2ccd2a9d385ed` |
| Same directory `root-ready-source-kernel-nedb-1` | `c40f3a1e5ee1a6cd5671930bd35e5a72ac36af9ce5daa0b2292656555e1eb3d0` |
| `native-main-startup-key-visitors-v13/root-ready-key-visitors-1` | `f47629e53fe11276e68c0805eba999b205f8ab1c34eca95a290e1001cbcd76f0` |
| `native-main-startup-notification-order-v13/root-ready-notification-order-1` | `6e9430fb95b39276009806416b3d139b69d92cf2317751ed023a181b0f6ebc85` |
| `review-main-startup-source-kernel-v13/root-ready-review-1` | `17c4b6e15f025a304d146d439b4f7587b00e07324898c85994cfceb59fdad0c2` |

**Use the notification-order successor.** The older 0768/c40f queues emit Unhandled by Promise creation order and HandledLater too late. Independent genuine Node/Rust reproductions are preserved. The new successor retains rejection-occurrence FIFO and emits HandledLater first; author log says **45 metadata tests + strict Clippy pass**, while its immutable manifest mistakenly says 43. Read `work/native-main-startup-notification-order-v13/CHECKPOINT_COUNT_ADDENDUM.md`. This correction has author metadata evidence, not a peer rerun or financial-Core/full-scheduler validation.

Original causal evidence: `native-main-startup-rpc-order-v13/root-ready-rpc-order-causal-1` (`dc7d6dbf…`), `root-ready-promise-notifications-1` (`1c95c2b7…`), and `review-queued-rpc-adoption-v13/root-ready-review-1` (`afdf1b3b…`). Use complete source observations as comparisons, never an executable schedule.

## Tools and checks

Use native Windows PowerShell and pinned Rust **1.90.0 MSVC**. `work/cargo-check.ps1` installs the process-local MSVC/SDK/Rust environment; invoke it while located in the intended repository. Bundled runtimes:

```text
Rust: C:\Users\patri\.rustup\toolchains\1.90.0-x86_64-pc-windows-msvc\bin
Node: C:\Users\patri\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe
Python: C:\Users\patri\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe
Target directory: C:\Users\patri\Documents\Codex\2026-10-26\go\work\main-v12-target
```

Runner environment and affected commands are in `work/check-outer-intermediate-second.ps1`. Its completed logs are guarded against overwrite: create a **new runner/log prefix**, do not rerun it as-is. Set writable TMP/TEMP and `OUTER_BOOTSTRAP_TMP`, the pinned JS root and bundled Node PATH. Supply-policy needs Rust on PATH; an initial missing-rustc environment failure was corrected and the actual policy passed. Prefer `rg` and real include-file enumeration, not Windows shell wildcard paths. An empty formatting list is not coverage.

No Cargo run is live. Completed handles include 6219 (26 checks), 50299 (exact committed cold12), 57067 (exact committed orderbook13), 56685 (cold12+strict lint), and 52099 (older39+public+lint). Do not restart completed broad regression93155. Heavy build outputs are reusable but are not evidence without matching input hashes.

Before any future **authorized** Rust push, run `bash scripts/ci-preflight.sh --full`; GitHub-only Lean/Kani/audit gates remain separate. Nothing is currently authorized to push.

## Completeness and end state

Saved workbook: `C:\Users\patri\Documents\Codex\2026-10-26\go\outputs\completeness-6b9c6c5f\TradeLayer-completeness.xlsx`. Sidebar artifact `artifact_g5Fs57Ux0BfP2n9VYK2wTTRQEDW2QMDU`, version195, last synchronized **2026-10-30T15:24:50Z**. It correctly records committed cold12/orderbook5 and the unchanged release baseline. The later 18 working checks are in this handoff/evidence, pending lint/commit; no new spreadsheet promotion is claimed.

Preserve all rows, formulas, filters, panes and layout. The supported builder is `work/build-completeness.mjs` using the bundled artifact tool; **REFRESH_ONLY is mandatory even for render-only**, or its creation branch can overwrite the book. Outside Codex, do not claim the live sidebar was synchronized unless actually updated. Spreadsheet automation is paused to conserve Codex.

After startup/cold integration, complete broader reference domains and transaction acceptance/authentication, native process-loss/reorg/recovery validation, fresh all-target/reference promotion, fuzz triage, intended-host real-block CPU/RSS and durability checks. The planned oracle-abuse/fraud-proof exercise must check TL's state-oracle output into the separate UTXORef consumer. Full reference parity, authenticated history and complete-block proofs remain unestablished. Follow the original roadmap, not a narrower success definition.
