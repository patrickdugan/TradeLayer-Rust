# Explicit rich-value source runtime

`RichThrownV14` adds an opt-in raw JavaScript value carrier for
`Main.getCurrentBlockHeight`, `Main.checkNetworkStatus`, and
`TxIndex.fetchChainTip`. It executes the existing financial/index owners and
preserves source rejection values, realm identity, aliases, cycles, UTF-16
strings, ordered effects and ordinary/array RPC success cloning.

The default remains `GlobalClearingV2`. Earlier profiles reject rich commands
before execution, legacy typed financial invocations remain closed under V14,
and Native13 continues to require V12. New outer `TLCR` tag14 and inner
`TLRC`/`TLRO` tag14 frames remain separate from earlier diagnostic formats.
Replay starts from recorded commands and reexecutes the actual owners.

Local validation passes 16 graph/protocol Core tests and two actual Core/replay
tests. The latter compare 46 complete original-source result/throw graphs and
RPC prefix snapshots. Existing replay, byte/version, native-owner and witness
regressions pass across 12 compatibility targets. Nine source regeneration
commands cover installed and locked numeric dependencies, full/proof corpora,
and 320 native intrinsic controls. Cargo formatting, 116 included files and
all-target Clippy with warnings denied pass. Exact counts, input hashes and
retained failure logs are recorded in
`fixtures/reference_fidelity/rich-runtime14-core-integration.json`.

This focused integration is excluded from the last full-release total of 1714
Rust executions and 238 numeric-reference checks at 963aae7. Two genuine source
partial-write owner paths, broader financial/deferred catch handling, richer
structured-clone success domains, arbitrary callables, native stack reads and
broader recursion/numeric domains remain unfinished. Native timer/file/row
continuations and cross-file durable recovery remain separate integration work.

The rich heap stores language/IO values; it creates no second financial Ledger
or metadata index. Its state commitment covers separately accessible material
and the same financial owner. Codec validation, checksums and hashes establish
consistency; they do not authenticate chain history, inputs or actors, and they
do not establish a complete-block fraud proof.
