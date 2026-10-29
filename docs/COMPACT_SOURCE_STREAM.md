# Generational source index and fixed Main streams

This additive continuation of c49cfb3 supplies one native metadata authority,
immutable read generations and explicit complete-stream profiles. It does not
replace V12 Core.tables, enable a Main policy, change TLCR/TLSW or enable economic
admission. Root owns actual integration, whole-Core gates and commits.

## Storage and publication

`CompactingJournal` wraps unchanged TLDB1 Journals inside a private catalog with
one active CURRENT and stable OS writer lease. Only txIndex is admitted. The
component copies no Ledger/Tally balances. A replacement Journal is an
unpublished candidate until CURRENT publication.

`TL-COMPACT-JOURNAL-GENERATIONS-4` is an explicit new catalog format. It copies
the healthy owner's actual page Arcs in bounded native batches, checks exact
copied KV equality, syncs the generation and metadata, then syncs and atomically
replaces the small unlocked CURRENT pointer. It never renames/truncates the live
locked Journal. A real Windows probe rejected hard-linking that locked file;
no weaker in-place protocol is substituted.

Plans bind the exact live-owner token, canonical catalog, persistent identity,
publication and logical sequence. Independent publication pins distinguish
separate stores with equal namespace/counters. Identity is local context, not
authentication, entropy certification or credentials. Relocation requires
explicit migration. Flat/older prototype stores are not silently reinterpreted.

Logical creation and physical creation sequences are separate. The exact peer
regression—two valid commits/live124MiB requiring three sorted copy batches—now
compacts, reopens at logical2 and continues at logical3. Cross-store and later
reopened-owner plans refuse before any mkdir/write/retention.

Each source DB operation stays one publication. Multi-write source methods keep
their original partial prefixes; MaxHeight/indexExists is not a completeness
certificate. Invalid/stale mutations refuse before maintenance. Conditional
commits prevent double-applying an old lost-acknowledgement retry.

## Read leases, crash and retention

Healthy snapshots share immutable page bytes and hold actual shared OS READERS
leases. Retention checks these leases, including after writer drop/reopen. It
retains the active generation, configured ancestors and leased generations.
Cleanup checks ordinary files/directories, reparse points, exact layout and
canonical containment. It removes named generated files rather than recursively
deleting arbitrary directories. Interrupted cleanup and orphan copies retry.
Generation/orphan, batch, byte and live-reader bounds remain explicit.

Tests execute nine actual child process exits at generation start/mid-copy/sync,
before/after CURRENT and the directory-barrier position, and before/during/after
retention. Reopen uses the independently expected old or issued candidate pin.
Ambiguous post-publication errors poison the owner; no data/snapshot/write is
served until reopen. Native partial-write repair and actual lost acknowledgements
after sync are also tested. There is no automatic alternate-publication fallback.

Unix uses ordinary directory sync. Windows certifies file sync/process-crash
behavior only: injected barrier failures and process exits do not prove Windows
power-loss/directory durability. Existing root/parent durability obligations stay
explicit.

## Source-compatible query streams

`TL-TXINDEX-SOURCE-MEMORY-PAGED-GENERATIONS-2` retains the bounded source-memory
storage domains of c49cfb3. Promise storage becomes an empty object; ambiguous
logical Error/BigNumber/prototype/date domains refuse locally. Tagged restart
semantics are deliberately different from lossy NeDB JSON disk encoding.

`TL-TXINDEX-COMPLETE-ROW-STREAM-2` records complete row count, hash chain, expected
metadata publication and completion flag. It never represents a shortened
findAsync array as complete and authenticates no actor or execution. Old array
APIs still refuse aggregate SourceValue bounds.

Main finishes its initial query and whole grouping before processing any TX.
The stream planner preserves that barrier: a late malformed payload prevents
earlier financial callbacks. It retains empty-payload height buckets, NaN-height
suppression, exact bounded height prefixes/hex, array coercions and funding4/20
before ordinary rows, with NeDB string-_id order within each phase. It stores row
locators and immutable page Arcs, not one enormous SourceValue array. Over4096
groups and heights beyond the exact supported numeric domain refuse locally.

The validity stream preserves the **fresh** /^tx/ query and first literal suffix
match. Its complete query receipt precedes the existing exact update/error path.
It covers duplicate suffixes, array/negative-zero IDs, coercion after the query,
matched/unmatched invalid reasons and1027 matches without an array proxy.

Seven controls execute unchanged complete Main and genuine NeDB1.8.0. A
transparent processTxSet observer delegates the original method and supplies no
financial result. The1025-row case yields684 funding then341 ordinary rows. A
later live-RPC height is explicitly unavailable after source processing; no live
success is fabricated. An initial discarded experiment lacked that boundary and
entered the source's unbounded virtual-clock loop; its OOM is not conformance or
performance evidence. Seven further controls execute unchanged TxIndex with
genuine NeDB. Both corpora regenerate identically with BN9.3.1 and9.1.2.

`propose_main_hooks.py` is a pure, unmounted proposal. It leaves old array limits
and factors the identical row/error body into an iterator over the SAME Core.
The native index loan, explicit new receipt/replay profile, original block
postphases and startup/file-map integration remain Root integration gates.
Neither V12 nor old aggregate receipts implicitly gain streaming support.

## Measured cost and bounds

The same4000 source-shaped rows/2000 source overwrites are measured before/after.
The old adapter retained205,555,686 Journal bytes for2,557,494 live bytes. The
initial generational run retained11,574,992 catalog bytes, with3,198,127 active
Journal bytes and two generations. Confirmed append205,585,669 plus checkpoint
copy57,094,726 bytes means approximately17.8x less retained file data but27.8%
more write IO. This is retention evidence, not a real-chain throughput speedup.
The final schema4 run retained11,575,643 catalog bytes (two generations), with
the same3,198,127 active Journal bytes and2,557,499 live bytes. Append/copy counts
stayed205,585,669 /57,094,726; the651-byte identity/location overhead is included.
The final schema4 measurement is separately recorded in the frozen manifest.
Counters reset on open and are observability, not replay/publication authority.

Rotation handles the512MiB per-Journal log ceiling rather than raising it. The
128MiB live-data limit,4094 pages,64KiB row encoding, bounded copy batches and
active-read retention still constrain storage. Real-history bootstrap, intended
host CPU/RSS/latency and hardware durability remain separate acceptance gates.

## Evidence and mounting

Standalone Rust1.90 uses immutable provider a82936bd. Current full checks pass
23 tests/one ignored child helper in debug/release, both Clippy checks, with the
child helper actually exercised at nine crash points. The final manifest binds
exact sources, providers, fixtures, source regenerations and measured growth.

1. Add exactly the owned payload files.
2. Apply `prepare_storage` to fresh storage.rs; it only appends healthy immutable
   snapshot and mutation-prevalidation methods. TLDB1 bytes stay unchanged.
3. Apply `prepare_lib` to fresh lib.rs.
4. Root runs actual unit/compatibility/release gates and commits reviewed paths.
5. Compose the separate Main proposal only with explicit native ownership and
   receipt/replay context, never two writable index stores.

No Root edit/Core build, push, PR, supplied financial snapshot, caller actor
verdict or authentication inference from a hash is part of this work lane.
