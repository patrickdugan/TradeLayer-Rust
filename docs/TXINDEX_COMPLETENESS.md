# txIndex completeness boundaries

The parallel scanner and literal source metadata owner are implemented as
separate components. Their integration and performance promotion are separate
gates; neither is a claim of authenticated Bitcoin input.

| Component | Implemented scope | Remaining gate |
| --- | --- | --- |
| Parallel scanner | Bounded fetch, fused raw parsing and TXID/wTXID checks, TL candidate extraction, deterministic source-order reduction and optional dependency waves | Compose source metadata writes under an explicit execution policy |
| Literal `txIndex.js` owner | All 21 methods plus constructor/singleton, client and DB initialization, MaxHeight/recovery flags, row insert/update/lookups, validity updates, source clear/reset errors and sequential RPC scans | Broader JS/RegExp domains and production backend integration |
| Generational native index and explicit Main session | Fixed complete-read streams, same-owner indexed and direct/live processing, native validity writes, pinned fresh-owner command-archive reexecution and retained source-error prefixes | Native startup/block routing, timer continuations, cross-file intent/ack recovery, full-history/resource support and host validation |
| HashMap reducer candidate | Externally owned draft PR141 and its existing synthetic evidence | Representative real-block CPU and peak-RSS measurements on the intended host |

The native integration passes232 focused Rust executions across16 targets and13
source-check commands. All seven native integration tests and163 library tests
pass, as do legacy byte/version checks, formatting and all-target Clippy. This is
an explicit additional session over the existing financial owner and one native
metadata authority. The subsequent full release963aae7 passes1,714 Rust
executions and238 numeric-reference checks with all1,422 frozen inputs unchanged. It does not automatically replace the default scanner or
establish full source parity or authentication. See `NATIVE_MAIN_INTEGRATION.md`.

The metadata owner is `TxIndexSourceOwner`. Its unchanged-source oracle covers
320 observations, of which 319 compare supported Rust execution and one records
an explicit local refusal of arbitrary RegExp injection. The genuine NeDB
reference includes memory/disk differences, invalid keys, duplicate-key behavior
at the backend boundary, source retained errors and retries. See
`TXINDEX_SOURCE_REFERENCE.md` and `TXINDEX_SOURCE_OWNER.md` for exact scopes.
Integrated Rust verification is recorded in
`fixtures/reference_fidelity/source-index-distribution.json`.

`processTransaction` really invokes Types with three arguments and does not
await the returned Promise. Rust preserves that original call and its unused
settlement rather than supplying a decoded success. Ordinary NeDB persistence
copies the Promise to an empty object. Source MaxHeight and startup rejection
suffixes are compared after draining the declared reference boundary. Exact
return-time/intercommand concurrency and production host survival after an
unhandled rejection are outside this component API.

A TLTI checkpoint restores the source owner's cache and singleton state.
External database rows, RPC facts, failure schedules and an independently
expected replay head belong to a composing runtime. A CRC or hash alone does
not establish execution history or authentication. Local owner rollback cannot
undo arbitrary side effects already performed by a generic backend; atomic
composition requires a transactional or candidate backend.

PR141 was last checked open, draft and unmerged at head
`8cb31416adf36dc7eefa53a396a40659eb3a6668`. The external worker retains ownership
of that lane. This port does not change its scanner or reducer implementation.
Bitcoin parent-UTXO, script and chain admission are separate authenticated-input
gates, not missing scanner code. TL emits checkable state-oracle data; UTXORef
is an external consumer.

## Current storage direction

Merged PR142 (`41007c5`) supplies the actual cookie-authenticated Bitcoin Core
bulk raw-block bridge and bounded Rust candidate consumer. This integration
uses that merged code. The compact-index research is tracked by PR143 against
upstream Bitcoin Core36629; it is not a TL dependency or a whole-output-script
prefix scan. Varying TL OP_RETURN payloads still use complete raw blocks.

External draft PR144 supplies durable SQLite candidate staging with independent
transaction/Merkle binding; draft PR145 explores immutable native Rust segment
publication. They are candidate-evidence storage lanes. Neither is a general
NeDB metadata replacement yet. The new literal owner has no Rust NeDB runtime:
genuine NeDB is used only to compare the old implementation in tests. The
production direction is an explicit compact candidate-store adapter, followed
by logic execution over verified inputs, with no migration back to NeDB.
