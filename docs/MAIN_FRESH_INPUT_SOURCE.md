# Fresh factual input: unchanged-source controls

This corpus executes ten original Main callers/interleavings under both genuine BigNumber9.3.1 and9.1.2. Pinned JS commit is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The driver composes the exact existing observer prefix (`native_main_service_tail_observer2_controls.cjs`, canonical SHA256 `eccac0f9e43b18d64c61fbf73e7523361da95a6b9c85be619053c05d6c32796c`), changing only its stage-relative base path. Main/TxIndex/whole TxUtils callable bodies and original Types/financial dependencies are unchanged.

Every scene uses one original Main/TxIndex instance, one actual NeDB native database and one existing financial environment. Raw updates change the dependency's factual IO table/documents; they neither reset/restart the source caller nor overwrite captured Promise values, counters, native rows or finances. The complete before/after observations include all named financial rows, native rows and actual native operations, ordered effects, file facts, receiver identity state, RPC counts, timers and promise-resolution status. Raw reset/constructor inputs/initial documents/command/update are separate records or explicit `rawControl` fields. Observed source values are never setup input.

Ten cases:

1. A captured tip11 Promise survives raw tip12; a later original call sees12.
2. Captured active network data returns its old status; a later call sees new inactive data.
3. Captured old block hash survives replacement facts; the subsequent old-hash lookup now returns undefined.
4. Captured block11 survives replacement facts; a later fetch sees the new block.
5. A genuine ETIMEDOUT fetch waits1000ms; the same retry sees fresh block facts and preserves RPC ordinals.
6. Genuine recovery waits10000ms on inactive network; the same caller sees fresh active network facts after its real timer.
7. An actual Incoming waits10000ms after block11; feeding tip/block12 causes the same source loop to process12 and wait again, without restarting.
8. A held raw transaction returns its captured bytes; removing its raw/document mapping makes the subsequent decode lookup absent. The original source swallows/skips that transaction and creates no native row. This partial result is retained, not repaired into success.
9. A held decoded-document Promise retains its captured document while subsequent lookups see the new raw document data.
10. Actual financial Main0 work/processed=true precedes a held real native validity update. Raw tip/block/document replacement leaves that native await, financial prefix and decoded row intact; releasing the same NeDB Promise produces valid=true once. A later original tip call sees12.

All four generation/check runs pass,44 full records per numeric profile. The two complete fixtures are byte-identical (SHA256 `5b191c9432d9d1e30192e212397a25a652dbd0295bd1838ab3f1305f4f26345e`). No Rust feed admission or cold/WAL replay is claimed by this source-only milestone. Initial cross-realm assertion and incorrect raw-future-lookup assumptions are preserved as failed evidence; only observer assertions were corrected from executed facts. No class body was changed.

Reproduce with `python check_source.py --repository <repository> --javascript-root <pinned-tradelayer.js>`. Optional `--generate` regenerates each fixture then checks it. This stage does not edit Root, run Cargo, push or commit; Parent owns integration/commits under the explicit lane instructions.
