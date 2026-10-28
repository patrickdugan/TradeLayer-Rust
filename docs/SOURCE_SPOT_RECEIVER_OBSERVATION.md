# Complete source receiver observation — v2

The first actual V9 library build exposed two observer omissions. Original
`Logic.cancelOrder` always constructs an Orderbook before selecting a branch.
The earlier standalone observer registered instances only when an observed
method ran, omitting the genuine receiver when the missing plural method threw.
The earlier Main observer registered loaders, omitting a receiver used only by
the original wrong-arity save. The Rust arena correctly retained those objects.

The new explicit `--all-receivers` observation mode wraps only the completed
CommonJS Orderbook export with a constructor Proxy. Its construct trap calls
`Reflect.construct` on the unchanged original class, checks the original
prototype and constructor, registers that exact returned receiver and returns
it unchanged. Internal lexical class constructions remain registered by their
actual original loaders. Source constructor arguments, method bodies, return
values, exceptions, balances and collection results are not replaced.

This is observation of the admitted graph, not a claim of unrestricted Proxy
function-identity equivalence for arbitrary external JavaScript. The original
frozen observation mode and its 64/27-frame files remain byte-identical. New
`*_receivers_v2` fixture names carry the complete receiver observation; the new
Rust tests select that mode explicitly and retain every receiver assertion.

Independent comparison confirms exact command/result equality, all financial
state equality and ordered effect equality after removing only observer
receiver identities. The comparison proof is included in
`SOURCE_SPOT_RECEIVER_OBSERVATION_PROOF.json`. Both genuine BigNumber versions
regenerate the corrected evidence identically. Original pinned source bodies
and source class/method hashes remain unchanged.

All three standalone and both Main tests passed after test-only linking to the
copied actual Root Rust 1.90 V9 library, SHA-256
`03a41e3aaf9ec50108f8eaf7a529bbefd415bb578e5ed89185a7339bcdf70982`.
The worker did not rebuild the library or edit Root. Root still owns applying
this observation/test correction and completing the required broader gates.
