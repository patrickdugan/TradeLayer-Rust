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

- Commit checked work to a branch and push that branch (user's standing rule, 2026-10-31). Never push to `main`, merge, or open a PR unless the user asks. Stage by path only.
- **NeDB is not consensus** (user, 2026-10-31). Do not model NeDB 1.8/async 0.2.10 executor turns or JS startup interleavings that depend on them. Storage is the Rust fast store (compact index, durable candidate store). Consensus parity is the state produced by processing blocks in order, compared on state outputs.
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

### Same-Kernel Promise/NeDB mount (committed)

The c40f/f476/6e94 components now live in Root as a cfg(test) domain of `MainStepKernelV1` (32 controls pass in Root, full library 838 passed/4 ignored). Production consults `startup_source_profile_active()`, constant false outside tests. Remove that gate only when a production containing controller exists. Mount script and gates: `work/claude-source-kernel-mount-1/`. Evidence: `fixtures/reference_fidelity/main-startup-source-kernel-mount-integration.json`.

### Outer frame bound to the same Kernel (committed)

The binding lane's native raw-request capture (anchor fixed for the current one-line visitor call), its outer hooks factor and hook/access leaves are mounted with Claude's driver `NativeMainOuterKernelDriverV1` (`src/source_main_outer_bootstrap_kernel_driver_impl.rs`, test domain). Microtasks drain fully, then one NeDB immediate runs. The outer request order equals the genuine projection (`fixtures/startup_outer_request_order_genuine.txt`). Inputs and gates: `work/claude-outer-kernel-binding-1/`. Evidence: `fixtures/reference_fidelity/main-outer-kernel-binding-integration.json`.

Result awaits (FindMax/Tally load/MaxProcessed) now subscribe at the async call's return and attach the actual result later (`fixtures/reference_fidelity/main-outer-kernel-early-routes-integration.json`). Known simplification: Request-time rejections deliver their callback at registration without an executor job.

## Startup ordering (decided 2026-10-31)

Startup order is deterministic. The committed literal nested drive (`src/source_main_startup_nested_runtime_impl.rs`) parks the first live block at its first native write, runs the outer `constructConsensusFromIndex` on the pre-write index, then releases the block. The outer read therefore sees the same three txIndex rows as genuine JS, without any NeDB model.

- The uncommitted queued-startup proposal (genuine queued-IO interleavings; its two positives failed on NeDB ordering) is retired. Its exact files are preserved on the local branch `archive/queued-startup-proposal` (`fb16f7f`), and the checkout is clean.
- The committed cfg(test) `StartupPromiseQueue`/`StartupNedbQueue` model and the outer Kernel driver stay as test-domain code. Do not extend them.
- Local branch `feat/startup-legacy-height-parking` (`6e75f40`, not pushed, not for merge) parks the legacy Incoming/Recovery height reads and writes on that queue: 5 controls pass, and the full preflight passed at that commit (306 targets, 2,649 tests).
- Remaining known gaps, in order: (1) parent UTXO/all-input/chain and economic/proof-verifier admission and complete-block proof composition; (2) mode4/inline/recovery/supersession histories and actual process-loss financial recovery; (3) intended-host power-loss and representative real-block CPU/RSS on the fast store; (4) opted-in fee/reward policy and provenance; (5) full-release promotion and collaborator fuzz review.

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

Before pushing a branch, run `bash scripts/ci-preflight.sh --full`; GitHub-only Lean/Kani/audit gates remain separate. Its last step (`scripts/audit_native_wallet_order.py`) needs `python3` on PATH and calls `cargo +1.90.0`, so put the rustup proxy (`C:/Users/patri/.cargo/bin`) ahead of the toolchain directory; the runner `work/claude-run-rustup.sh <export> <command>` does both.

## Completeness and end state

Saved workbook: `C:\Users\patri\Documents\Codex\2026-10-26\go\outputs\completeness-6b9c6c5f\TradeLayer-completeness.xlsx`. Sidebar artifact `artifact_g5Fs57Ux0BfP2n9VYK2wTTRQEDW2QMDU`, version195, last synchronized **2026-10-30T15:24:50Z**. It correctly records committed cold12/orderbook5 and the unchanged release baseline. The later 18 working checks are in this handoff/evidence, pending lint/commit; no new spreadsheet promotion is claimed.

Preserve all rows, formulas, filters, panes and layout. The supported builder is `work/build-completeness.mjs` using the bundled artifact tool; **REFRESH_ONLY is mandatory even for render-only**, or its creation branch can overwrite the book. Outside Codex, do not claim the live sidebar was synchronized unless actually updated. Spreadsheet automation is paused to conserve Codex.

After startup/cold integration, complete broader reference domains and transaction acceptance/authentication, native process-loss/reorg/recovery validation, fresh all-target/reference promotion, fuzz triage, intended-host real-block CPU/RSS and durability checks. The planned oracle-abuse/fraud-proof exercise must check TL's state-oracle output into the separate UTXORef consumer. Full reference parity, authenticated history and complete-block proofs remain unestablished. Follow the original roadmap, not a narrower success definition.
