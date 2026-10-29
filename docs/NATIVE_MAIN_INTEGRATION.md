# Native indexing and Main session integration

The compact index now has an explicit moved-owner Main session. It retains one
financial replay and one native metadata index. The existing default clearing
policy and the V1–V12 replay formats keep their previous behavior. Session13 is
an outer ownership and receipt format; it does not add a global Runtime policy13.

`SourceMainNativeSessionV13` consumes the existing owners after checking their
financial head, network namespace, metadata snapshot and Main client. A Core
containing a second txIndex collection is rejected. The native Journal stays
outside cloned financial state. Its scoped callback routes successful and fatal
validity updates to the same native index while the existing Main row body changes
the same Ledger/Tally/registry owners.

The native query path records complete reads rather than truncating a NeDB query
into a bounded array. Main completes its initial grouping before processing rows,
preserves funding-before-trading order and retains genuine source exceptions and
partial financial writes. Direct `ProcessRows` retains the original bounded
caller-array interface for `processTxSet` and live `processTx`.

The TLCR13 archive stores the pre-native source-command foundation and the actual
new commands and receipts. Reexecution rebuilds financial state by commands into
a fresh, explicitly pinned raw index. It never imports a supplied current
financial snapshot. Current session heads include financial and native logical
receipts; the old foundation checkpoint is explicitly named and cannot be exposed
as the current replay. Local physical identity, publication and location bindings
are checked separately from logical replay identity.

The underlying generational journal publishes a compact replacement through a
small CURRENT pointer while immutable read generations retain OS reader leases.
Logical and physical creation counters are distinct. Invalid foreign/stale pins
are rejected before incomplete-tail repair. Existing TLDB1 bytes are unchanged.
See [storage details](COMPACT_SOURCE_STREAM.md) and the
[rejected-pin correction](COMPACT_REJECTED_PIN_REPAIR.md).

## Verification and scope

The main-checkout integration passed **232 Rust executions** across16 targets
(231 top-level tests and one successful storage subprocess), including163 library
tests and all seven native integration tests. All-target Clippy, Cargo formatting,
114 explicitly included implementation-file formatting checks, the zero-crate
supply-chain gate and the per-type inventory check passed. Thirteen source-check
commands passed, including default and locked numeric regeneration and four
genuine host/VM Array-length controls. Exact input and log hashes are recorded in
`fixtures/reference_fidelity/native-main-core-integration.json`.

The subsequent full release963aae7 passed **1,714 Rust executions** across270
targets and **238 locked numeric-reference checks** across86 targets. All1,422
frozen inputs were unchanged. This supersedes the previous9ff4aab full-suite
baseline while preserving all remaining integration gates below.

The isolated actual Core probe4 passed seven native integration tests and seven
legacy compatibility checks under Rust1.90 MSVC, plus all-target Clippy with
warnings denied. The native cases compare the original JavaScript with actual
Main0 property, tally, activation and consensus changes, complete1100-row reads,
late grouping errors, fatal validity fallback, direct/live returns and fresh-owner
archive reexecution. Earlier probe2 separately passed162 library tests, including
20 new storage/stream tests and eight rich-graph foundation tests. These are
focused checks; they are not a new full-suite release count.

The unchanged Main/TxIndex modules execute with genuine NeDB1.8.0 and default
and locked BigNumber9.1.2. Main grouping and validity each have seven independently
regenerated source controls. Five additional native Main controls preserve11051
JSONL frames. The number of fixture frames is not a count of independent calls.
Repository-local reproduction commands and input pins accompany the generators.
The catchup controls retain their explicit later live-RPC boundary; they do not
invent successful execution of an unattended incoming-block loop.

The bounded TLRV1 graph codec is an independent foundation for future rich thrown
values. It preserves UTF16 strings, number bits, object identity, descriptors and
native realm information. Its standalone format does not enable Runtime14 or
make codec-only observed callables executable capabilities.

## Remaining integration

Native initialize/sync/block routing, suspended timer continuations, cross-file
write-intent/ack recovery and broader financial resource continuation remain open.
The session halts on uncertain native IO or a local resource stop. Its command
archive alone is not a durable transaction protocol across native files and the
financial history.

Recovery must resume the exact original continuation. The pinned source performs
six settlement/clearing phases for fresh processing, but zero if a naive retry
starts after the transaction's processed flag was written. Reentering Main after
that prefix would therefore change behavior. The source control and continuation
design are retained for the next implementation stage.

Windows file synchronization and actual process-crash tests do not certify
directory durability under power loss. Real-history bootstrap and intended-host
CPU/RSS measurements remain separate gates. Existing explicit row, group, graph,
archive and live-data resource bounds remain visible.

This integration does not establish full reference parity, authenticated L1 or
actor history, a complete fraud proof, or authorization from hashes. TradeLayer
produces checkable state-oracle data; UTXORef remains a separate consumer.
