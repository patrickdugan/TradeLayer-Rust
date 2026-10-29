# Recovery checkpoint — 29 October 2026

This is a saved continuation checkpoint, not a completed port or crash-recovery claim.
Do not push, publish or open a PR without the user's explicit request.

## Committed baseline

- Latest committed implementation: `ed74e1693a5db9ad03fb35d1eeb79955387d6bc2`.
  Main saved-height/genesis error continuations preserve the same owners, raw
  rethrow/swallow behavior, writes and ordered effects. Three Root Core tests,
  29 compatibility executions across 13 targets, four original-source numeric
  checks, formatting and all-target Clippy pass.
- Last full release: `963aae732a1274bb4dafacd24e9d6666451ac51c`, with 1,714 Rust
  executions and 238 numeric-reference checks. Later focused results are not a
  new full-suite pass. Default `GlobalClearingV2` remains unchanged.
- The workbook and sidebar record these scopes separately. Per-type full
  reference parity and complete-block fraud-proof classifications remain open.

## Tested native work, still outside Root production integration

- Corrected native controller/segmented replay: 18 actual Core tests. This
  includes 4,353 commands across 18 segments, fresh-owner replay and the unchanged
  35-command byte-pressure regression. The original failure is retained.
- Intent store: 16 Core tests plus one ignored subprocess entry; seven tests are
  new store cases and nine are reused controls. Eight real child-process exits
  test sync boundaries and strict rejection of unknown/torn state.
- Prepared validity: seven Core tests, including complete 1,100-row queries,
  actual opaque native plans, distinct read/write pins, stale/foreign refusal
  and exact post-reopen resolution without duplicate commit.
- Shared metadata factoring: 14 actual Core executions across four targets,
  including the original-source TxIndex corpus and prior rich-owner regressions.
  The factoring is not yet promoted into Root.

These checks do not prove financial Main crash recovery, directory power-loss
durability, full-history/resource parity or authenticated input/state. A single
receipt above the declared segment limit remains an explicit resource boundary.

## Accessible local package

The source/evidence archive and committed Git bundle are saved under:

`C:\Users\patri\Documents\Codex\2026-10-26\go\outputs\recovery-checkpoint-20261029`

`native-recovery-source-and-evidence.zip` contains the tested native source
checkout, the metadata factoring checkout, immutable stage packages, original
JavaScript oracles, successful and failed logs, mounting/build scripts, and the
saved completeness workbook. Its internal `PACKAGE_MANIFEST.json` records every
file's size and SHA-256. `PACKAGE_SUMMARY.json` beside this README records the
archive digest. Run `verify_package.py <archive-path>` before using a transferred
copy. The Git bundle is independently checked with `git bundle verify`.

All files are on this Windows filesystem. No browser/container download or
computer-use transfer is needed. Existing work folders remain intact.

## Resume order

1. Inspect Root status and this handoff; keep unrelated changes untouched.
2. Read the saved worker `WORK_STATE.md`, `ROW_RECOVERY_CONTRACT.md`, and the
   latest packaging addendum. Distinguish implemented/tested pieces from drafts.
3. Integrate the reviewed native pieces through their pure transforms. Do not
   overwrite current Root `source_contract_runtime.rs`, `lib.rs` or TxIndex files
   with the older whole-file snapshots: that would discard newer rich-value work.
4. Finish actual invocation begin/finish and sealed Main row/catchup program
   counters. Preserve source locals and the same borrowed financial queue.
   `source_main_invoke_with(rollback=false)` can stop before queue leave;
   resuming must not reenter invoke, decode again or mark processed again.
5. Bind the actual prepared validity read/write operation and synced intent/ack
   to that retained row. Preserve primary/fatal source branches and shutdown.
6. Finish verified complete-KV checking under the same lock before any tail
   repair, typed no-index ownership, superseding intermediate candidates and
   process-loss command/prefix reexecution. Never retry whole Main against
   current `alreadyProcessed` metadata or import a financial snapshot.
7. Execute original-source await/rejection controls, actual Core recovery/crash
   tests, old version/byte guards, formatting and all-target Clippy. Commit only
   reviewed task-owned changes. Run the full release suite before promotion.

Root owns whole-Core Cargo runs; use one at a time. Agents may use immutable
providers for standalone work. Existing runner logs are protected: use fresh
evidence names instead of overwriting failures. Rust is pinned to 1.90 MSVC;
the saved runners establish the local SDK/runtime paths.

TL exports checkable state-oracle data. UTXORef remains a separate consumer;
optional Binohash work belongs there. Checksums, hashes and store pins provide
consistency, not chain/UTXO/actor authentication.
