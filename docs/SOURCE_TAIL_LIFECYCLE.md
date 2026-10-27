# Literal rollup and transaction types32–35

This bounded V6 admission follows the unchanged JavaScript source at
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. It adds no zk verifier, proof
acceptance, bridge, colored-coin balance owner or contract-binding success.
The actual reached source graph supplies no such financial execution.

The reference loads complete pinned Logic/Types/txDecoder/txEncoder modules and
their genuine dependencies. It also pins the main.js archive used to identify
the caller's `decodedParams.valid === true` gate. Main/Consensus daemon execution
is not emulated. Source body hashes, absent methods and actual synchronous versus
asynchronous function declarations are recorded in the method inventory.

## Exact source outcomes

| Surface | Literal result |
| --- | --- |
| Raw32 Types decoder | `ordinalRevealJSON` retains body text; block/sender/txid added; no `valid` field or validator |
| Raw33 Types decoder | Prefix-parsed numeric fields and home address; block/sender/txid added; no `valid` field or validator |
| Raw34 Types decoder | Throws `Decode.decodeCrossLayerBridge is not a function`; existing `decodeAbstractionBridge` is a different method |
| Raw35 Types decoder | Throws `Decode.decodeSmartContractBind is not a function` |
| Direct `Logic.batchMoveZkRollup` | Synchronous Error for invalid JSON; supported valid JSON reaches synchronous ReferenceError for the unbound `isValidRollupDataStructure` identifier |

`Logic.isValidRollupDataStructure` exists as an object method. The rollup method
does not call it through `Logic` or `this`; no unqualified/global helper is
installed. Its verifier call is therefore not reached. The port preserves this
defect. Malformed JSON and coercion errors caught by the source try/catch become
the exact `Error('Invalid rollup data format')`. Values outside existing bounded
JSON/UTF-16 representation limits remain explicit local refusals.

The runtime reuses its existing original Types decoder and strict `valid ===
true` Logic gate. Consequently raw32/33 return their original parameters without
financial Logic execution; raw34/35 retain the source exceptions. No valid flag
is fabricated. `Logic.batchMoveZkRollup` is admitted as an explicitly direct,
synchronous dependency method. Its source Error is a `Throw`, not a rejected
Promise. V1–V5 keep their previous unknown-dependency/raw-type refusals and
checkpoint domains.

The wider audit separately records direct `Logic.typeSwitch` calls, including
its looser `params.valid == false` early return. Directly composing Types32/33
with that dispatcher produces different errors from the main strict gate.
Those profiles are explicitly distinguished. The runtime does not repair or
alias absent `Logic.coloredCoin`, `crossLayerBridge`, `smartContractBind` or the
decoder names. Existing `bindSmartContract`/`mintColoredCoin` stubs are audited
as original empty methods; they are not silently substituted into dispatch.

## Corpus and limits

The genuine audit has 131 observations under four module import orders. Both
archived BigNumber9.3.1 and independently checked genuine9.1.2 reproduce the
entire capture. It retains full source rows/caches, ordered observed function
calls, original arguments and return/error transports. Every observation checks
that the full source financial/cache state remains unchanged. No verifier or
financial result is supplied. Inspection warms only the reference observer's
empty collection views before taking its fixed baseline.

The first ten observations were checked before broader expansion. The expanded
corpus covers source JSON coercion, malformed/valid/nonfinite JSON, the actual
pure structure helper's shape and shadowed `hasOwnProperty` errors, loose
dispatcher guards, real decoder prefix parsing, non-string bodies, invalid
markers, missing exports, encoder/decoder mismatch and direct unused Validity
helper failures. Metadata-only pure helper returns do not authorize execution.

The focused runtime tests compare 49 admitted original operations, rather than
claiming the wider audit's unmounted direct methods are implemented. They check
exact outcomes, synchronous rollup transport, absence of DB/Tally writes,
checkpoint replay after every admitted operation, independently expected replay
head rejection and old-policy isolation. The original raw argument arrays and
source errors are journaled; no poststate or dependency verdict is imported.
Both focused tests pass against the coordinated final V6 runtime library.
Their embedded reference uses a shallow string-framed transport for diagnostic
values, so the generic JSON parser's framing depth is not confused with the
decoded SourceValue depth. The retained standalone audit still contains every
full source snapshot. Test Clippy and pinned Rust1.90 formatting checks pass.

Input caps and the JSON representation domain are inherited from the existing
literal runtime and parser. Functions, accessors, arbitrary module/global
replacement, a supplied verifier and unrepresentable UTF-16 values are outside
admission. Raw sender/block parameters and imported state remain unverified.
Replay hashes/CRCs prove conditional consistency, not chain, actor or custody
authority.

## Reproduction and mounting

The Rust target embeds the complete genuine-source driver and requires no scratch generator or captured work files:

```text
cargo test --frozen --test source_tail_lifecycle
```

The archive loader and independently checked BigNumber9.1.2 constructor verify pinned bytes. The batch validation receipt records full checks. V6 adds only the explicit raw/direct graph and synchronous transport; it introduces no verifier, global alias, balance owner or repaired source path.

Pinned integration validation: 1,449 Rust test executions across208 targets, 84 source-lock tests across31 targets, formatting, Clippy, doctests, release and Python policy/bridge checks pass. Exact scope, corpus counts and input hashes are recorded in `fixtures/reference_fidelity/source-proof-candidate.json`; these checks do not establish complete reference parity or authenticated block execution.
