# Genuine source mark settlement composition

`tests/source_clearing_global.rs` drives the unchanged complete
`fixtures/legacy_clearing_source.txt`, SHA-256
`612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528`,
from `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
The reference is the actual common live realm of ContractRegistry, Margin,
Clearing, Oracle, Tally, Orderbook, Options, IOU, and Insurance modules.
Only the declared DB, clock, UUID, random, and unavailable service adapters
remain external boundaries. Reference outcomes never answer Rust dependencies.

The corpus exercises `updateMarginMaps` and `makeSettlement`. Direct map-update
inputs derive from actual live `isPriceUpdatedForBlockHeight` and registry
reads in both implementations, preserving the source's mutable same-block
mark cache. No test invents a validated order or substitutes a supplied price
verdict. Whole settlement calls run their own actual registry and Oracle reads,
book pruning, IOU allocation, position updates, and additional insurance tasks.

Its 50 observations also include two failure/retry scenes restored after each
of eight observations through the complete original-command journal. The
expected head is retained independently; restoration compares whole current
owner state, object aliases, live contexts, and canonical bytes before the
retry continues.

Cases include empty positions, no previous mark, positive and negative linear
marks, inverse marks, options, fully insolvent and partial liquidation paths,
failed IOU writes, failed tally deltas, failed map writes, and retries. A retry
re-reads the actual mark cache, so a source exception can change the work
performed on a repeated call. The insurance distribution scene reaches the
source's missing `PnlIou.absorbDust` method after prior credits; its TypeError
and partial state are preserved rather than repaired.

After each observation, comparison includes full persisted collections, Tally
cache, every loaded live Margin instance, actual Orderbook receivers and cached
books, insurance instances, block trades, Oracle/native mark caches, liquidation
and deleverage maps, loaded contract-series metadata, modification flags, and
remaining clearing-context positions. The Rust context's object indices are
resolved through the actual Margin owner so alias-backed current values are
compared with the source's live position cache. Ordered DB, UUID, random, and
RPC effects retain original argument arity and thrown/returned results.

All 50 genuine reference observations also match exactly between the archived
BigNumber 9.3.1 constructor and the verified source-lock 9.1.2 preload. This is
corpus equality for these executions, not a general equivalence claim.

The test loader can load the genuine unchanged `legacy_vaults_source.txt`,
SHA-256 `3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983`.
Only its loader inventory changes; no vault implementation bytes or outcomes
are substituted. Top-level channel removal and supply reconciliation are
covered separately by the supply-owner corpus. This document does not claim
daemon, external chain authentication, or remote service completion.
