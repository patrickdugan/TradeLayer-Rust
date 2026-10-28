# Main lower-handler integration delta

This source-composed delta broadens the existing `MainCallableV8` transaction loop to
the already-owned raw kinds 1, 2, 3, 4, 7, 8, 9, 11-15, 18 and 32. It reuses the
same original Types/Logic factors and Core Tally, queues, Property, Oracle,
ClearList, Channels and actual SourceActivation registry. The existing passing
39-observation Main tests and legacy policy/version tests are retained.

The genuine lower corpus contains 38 frames: 25 Main calls, 11 constructor/input
resets, one raw transaction-document import and one explicit RPC binding. Of the
calls, 23 have available source dependencies and two are separate unavailable-RPC
experiments. The ordinary first-ten sequence has an explicit original TxUtils
binding and data-only raw document in its reset input. It includes successful
commitment and all six actual live phases. The default unbound experiments are
isolated by resets; neither their source poststate nor financial prefix is
injected as Rust prestate for later comparisons. Raw3's missing validator output
and original Main fatal-catch/processed behavior remain source observations.

The new test runs original Node modules under both genuine BigNumber versions,
compares complete collections, source caches, all Orderbook/Margin receivers,
ClearList's four true Maps and the distinct Tally singleton receiver. It retains
original RPC-read arguments/results, ordered DB and phase calls, source throws,
and failure/retry effects. Every admitted transition is reconstructed from its
complete replay journal with an expected head and exact checkpoint re-encoding.
Unavailable capability calls are explicit local refusals with whole-runtime
rollback; the test rejects continuing a source segment after one without reset.

The only source behavior delta is an explicit owned-kind admission extension;
other kinds, startup, full index catch-up, reorganization, Persistence and
production RPC remain unimplemented Main capabilities. The standalone explicit
raw-RPC document binding supplies inputs, not a pubkey, validity or authorization
verdict. Imported registry/balance/actor/block facts remain unverified inputs.
No hash establishes authentication or a complete block/transition fraud proof.

Status: parent-owned Rust1.90 Cargo validation passed all five focused Main/lower/version tests. All-target Clippy with warnings denied, formatting, inventory and four Node regeneration checks pass. The new test compares 23 actual calls under both genuine BigNumber versions and exact replay. The earlier full 1,475-test suite covers the source checkpoint before this guard extension. Combined release validation remains separate.

`prepare.py` applies only the admission-line change to the existing Main include.
The complete staged include also updates its scope comment. The portable Node
script changes its default reference-root path and adds `--print` transport for
freshly executed tagged records after fixture checks; all pinned original source
and class/method bodies are unchanged. Set `TL_MAIN_LOWER_EVIDENCE_ROOT` to this
stage for a work-only test executable, or install script/fixtures at the repo
root for the default test paths. Source fixtures are assertions, never runtime
dependency answers.
