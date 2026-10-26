# BitVM risk metadata counters

`BitvmRiskBook` implements all eleven functions of pinned `bitvmRisk.js`: `nenv`, `clampNonNeg`, `base`, global/DLC/window key helpers, `readOrInit`, `writeDoc`, `onCacheOpen`, `onEscrowRelease`, `onSweep`. Complete unchanged LF source SHA256: `14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66`, revision `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.

The module owns risk-reporting counters only, in the source's existing procedural category. It is not another spendable balance store. Recorded pending amounts do not prove that escrow is funded, and passing a risk cap does not authorize deposits, withdrawals, sweeps or claims against the sole Ledger.

An explicit environment snapshot supplies caps and window size. Missing, empty and nonfinite environment values use the source fallback. Caps at zero/negative disable enforcement. Window size floors and clamps to at least one; block values preserve source Number conversion, negative clamp and floor. Source NaN/infinite block indices can appear as literal key suffixes because the stored rows do not contain those nonfinite numbers.

All counter arithmetic retains source binary64 operation order. `on_cache_open` increments global/DLC pending and the current window's deposits, checking global, DLC and deposit caps in that order. `on_escrow_release` checks the window withdrawal cap and independently clamps global/DLC pending at zero. `on_sweep` returns without recording window state if its cap is disabled. Invalid property/amount calls are successful no-ops; the journal records these invocations for complete replay.

Source defects remain visible. Repeated calls count repeatedly. Over-release can leave global pending below the sum of DLC pending rows. Empty and literal default DLC references share a key. No counter is reconciled to actual custody or an independent cache journal. Floating-point thresholds and decimal drift are retained rather than silently replaced with atomic-integer accounting. Nonfinite stored results, including addition overflow characterized in the full-source corpus, reject atomically as an explicit finite metadata boundary.

The repair policy is `SourceRepeatCallsAtomicV1`: all affected rows and the journal commit together after validation. This fixes the source's sequential three-row partial-write behavior; it does not invent idempotency. Callers needing accepted economic event deduplication must supply it through their authenticated owner. No external I/O occurs during mutations.

Records/journal use the same documented bounds as the cache module:1024 records,4096 events,8MiB total serialized metadata,65536 value nodes, and per-document64KiB/4096-node/depth16 bounds. Timestamps are supplied JS-safe integer milliseconds. There is no automatic pruning that discards replay evidence. Versioned CRC checkpoints retain the complete environment, journal and sorted rows. Restore requires expected configuration, canonical fields, full replay and exact resulting state/bytes. Metadata checkpoints never become proof or custody capabilities.

The 48 full-source oracle observations execute the unchanged module with real Node crypto availability and explicit database/environment/time adapters. They cover all three entry points, key/window helpers, fractional/hex Number inputs, disabled and exact caps, cap error order, default DLC aliases, repeat/over-release effects, and a second-write failure leaving only the global source row committed. The Rust policy intentionally rejects that partial-commit outcome; its runtime tests verify staged failure, overflow, replay, configuration mismatch, repaired-CRC tampering and aggregate resource rejection.

Run `node scripts/bitvm_risk_vectors.cjs --check`; optional `--source-root` checks the actual pinned source. No BitVM proof, circuit verifier, signature verifier, token issuance or Ledger mutation is implied.

Restore and mutation preflight aggregate JSON nodes before journal copies; metadata budgets reserve room for encoded keys, configuration and framing. Input ordering, binary64 values and source quirks are retained inside these explicit resource limits.
