# Durable joint-chain application and rollback

`src/persistence.rs` owns one `Journal`, one complete `JointChainState` and its
bounded block-header index. A block is applied and finalized in a clone. The
complete resulting TLJC checkpoint and header changes are committed in one
synced journal transaction. Only then does the owner publish its new memory.
Rollback of one or multiple retained blocks uses the same transaction boundary.

This integrates the existing joint registry, spot exchange, sole spendable token
ledger, input replay sets and retained rollback states. It does not include the
new standalone margin, oracle, AMM, procedural, vault, vesting or IOU stores.
Those modules still require one integrated economic owner and complete shared
checkpoint. `DurableFundedIou` is a separate existing experimental state owner;
running it beside this owner does not create cross-namespace atomicity or a
single deployment ledger.

Every block, actor and transaction here remains an unverified caller assertion.
A correct local restart neither authenticates L1 consensus nor authorizes a
TradeLayer balance change. CRCs, local heights and the source's so-called
consensus hash do not provide that authentication.

## Ownership and reopening

`PersistenceConfig` contains independently supplied network, original anchor,
administrator, permitted spot properties, rollback limit and header retention.
`initialize` checks a complete candidate checkpoint against that configuration
and an independently supplied exact expected tip before writing. It only accepts
a journal with sequence zero and no live entries. An old journal whose entries
were later deleted cannot be silently reinitialized.

`load` calls `Journal::open_existing`; a missing store never creates a directory
or file. It checks the exact stored configuration, full TLJC decode, independently
expected tip, canonical header keys, header heights/links, and agreement between
the header index and every retained checkpoint transition. Unknown keys and
semantic corruption reject even when journal CRCs are valid. The original
anchor remains mandatory when reopening a bounded-history floor.

The current state, tip, configuration, sequence, individual headers and header
iterator all check journal health. After an uncertain seek/write/sync result,
cached reads and further mutations reject. A returned I/O error can follow a
fully synced commit. Drop the owner, inspect recovery and reconcile an
independently expected tip; retrying an economic operation blindly can duplicate
effects. Old caller-owned clones remain old unverified observations, not a way
to access a healthy live owner after failure.

The journal is private, and no mutable chain or ledger escape hatch is exposed.
Initialization may import an already progressed, internally consistent TLJC
state, with its retained headers reconstructed by read-only rollback of a clone.
That import itself does not authenticate its inputs or balances.

## Checkpoints, bounds and rollback floor

All keys live in the configured mainnet `persistence` namespace:

| Key | Value |
| --- | --- |
| `joint/config` | TLPM v1 expected network/genesis, original anchor, admin and retention/policy fields. |
| `joint/state` | Complete current TLJC checkpoint, including joint state and retained pre-block states. |
| `joint/header/HHHHHHHHHHHHHHHHHHHH` | Canonical 20-digit height key and 72-byte height/block-ID/parent-ID record. |

The supplied rollback limit is 1–64, and header retention is at least that limit
and at most 2,048. Applying one block prunes only the oldest header exceeding
that explicit retention bound. Rolling back deletes headers above the new tip.
Header metadata is not an independent mutable height tracker.

A valid rollback to the oldest retained state can leave an empty history above
the original anchor. TLJC's narrowly versioned floor representation preserves
that state without fabricating a predecessor or changing the original anchor.
The owner round-trips every candidate through the configured decoder before
committing it. An empty header index is permitted for this checked empty-history
floor: the floor's own parent header may no longer exist. Its exact tip still
comes from the checkpoint and the caller's independent expected-tip check.
Further rollback fails when history is exhausted; a new block can extend the
floor and create new retained history normally.

Journal limits remain 64 MiB per value, 65 MiB per batch, 4,096 keys, 128 MiB live
key/value bytes, 1,024 batch operations and 512 MiB total log. The owner writes a
full current-state checkpoint per accepted block or rollback; it does not claim
incremental checkpoint efficiency. There is no log compaction or rotation. A
full log rejects before writing and retains the previous healthy state. Large
states can exhaust that log in relatively few blocks. Production operation
requires a separately reviewed compaction/archival policy and measured memory,
disk and restart behavior on the intended host.

`Recovery::RejectIncompleteTail` preserves evidence. Explicit
`DiscardIncompleteTail` may truncate only the final incomplete transaction under
the journal's rules. This recovery happens while opening the journal, before
configuration/tip validation; selecting it authorizes that truncation even if
later application validation rejects. Complete corrupt frames never become
recoverable tails. Device power-loss, directory durability and lock limitations
remain those documented in `STORAGE.md`.

## Pinned JavaScript behavior and active-method map

The complete 557-line source is archived as
`fixtures/legacy_persistence_source.txt` from commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, LF SHA256
`628c5b3cfd919fd518aedc2b757cdc256f7ccc36c447090f30aebd8a677ce2e1`.
`scripts/persistence_vectors.cjs` executes the unmodified class with explicit
virtual POSIX filesystem, NeDB, RPC, clock and consensus-hash adapters. It never
copies or deletes real application databases. Its observations characterize
source behavior, including failures, rather than make unsafe restore decisions
for the Rust owner.

| Active source method | Rust implementation or evidence boundary |
| --- | --- |
| `getInstance`, constructor, `init` | Explicit owned initialization/configuration and preexisting root; source singleton/network initialization is exercised by VM. No hidden RPC or singleton is introduced. |
| `recordBlockHeader` | Headers are written only with a successfully staged complete block, not by an independent metadata mutation. Source upsert behavior is characterized. |
| `getBlockHeader`, `getLastKnownBlock` | `get_block_header`, `headers`, and the authoritative checkpoint `tip`; source query/upsert results are characterized. |
| `checkForReorgForNewBlock` | Source fast-path, stale metadata and RPC failure branches are executed. Canonical-chain observation and authentication remain caller responsibilities. |
| `detectAndHandleReorg` | Explicit atomic `rollback_tip`/`rollback_to` within retained state; source ancestor search and unsafe automatic snapshot selection are characterized separately. |
| `maybeCheckpoint` | Exact `source_checkpoint_due` integer-domain helper; actual owner durability occurs on every successful block. |
| `createSnapshot` | Complete existing TLJC state in one journal batch. Live NeDB file-copy behavior remains source characterization, not a migration implementation. |
| `cleanupPersistence` | `source_cleanup_before` characterizes the exact source threshold; runtime pruning follows explicit bounded header retention. |
| `updateMaxProcessed`, `saveTrackHeight` | No independent mutable height flags: committed tip and complete state are in the same checkpoint batch. Both source writes are exercised. |
| `restoreSnapshot` | Checked `load` and atomic retained-state rollback. Source backup/delete/copy behavior and missing await are characterized. |
| `listSnapshots`, `pruneOldSnapshots` | Source catalog sorting, malformed metadata and pruning are exercised. Rust retains bounded complete rollback states; it does not expose a historical NeDB snapshot catalog. |
| `safeCopy`, `safeDelete`, delay helper | Source bounded retry, missing-file handling, exhaustion and other errors are exercised. No directory-copy/delete API is promoted. |

The three public source helpers are diagnostic metadata compatibility only, on
the checked nonnegative JS-safe-integer domain. In particular,
`characterize_source_snapshot_choice` intentionally reproduces the source's
unsafe height choice, is never called by the durable owner, and restores no
data. Its fallback can choose a future snapshot. General JS coercions outside
the documented typed domain are not claimed.

## Concrete source defects preserved as evidence

Executed source observations show these behaviors:

- A second `getInstance` can return an object whose directories are still null
  while the first initialization awaits its RPC client.
- When canonical RPC matches the incoming parent but local metadata is stale,
  `checkForReorgForNewBlock` still returns a reorg object. Its purported
  false-positive resolution path is not reached for normal hash strings.
- An RPC failure calls undefined `logger.warn`; the outer catch returns false,
  so the method fails open rather than returning its intended uncertainty.
- With local tip 10 and common ancestor 5, snapshots at 3 and 8 select and restore
  snapshot 8. The source ignores the ancestor during selection. If every
  snapshot is above the local tip, it falls back to the first future snapshot.
- `restoreSnapshot` returns before the unawaited `saveTrackHeight` update
  completes. Pruning old backups before creating the next one leaves two
  backups after a restore.
- A missing snapshot file can fail restoration after the live `.db` files have
  already been deleted. The backup survives, but restoration has no atomic
  rollback. Source metadata has no bounded validated manifest or file hashes.
- Snapshot copies occur before `MaxProcessedHeight` is updated, and header
  cleanup uses `checkpointHeight - 1000`, despite a different comment/log.

Additional source/caller defects found by inspection remain integration gates:
`main.js` invokes `maybeCheckpoint(h)` before processing block h's transactions,
yet labels the snapshot and maximum height h and resumes at h+1 after restore.
The copied state can therefore precede its declared height. Live NeDB files
are copied without a coherent state barrier, and restored files are not followed
by closing/reloading the existing cached Datastore instances. Snapshot creation
reuses height/hash-prefix directories and writes metadata without a durable
atomic publication protocol. Snapshot metadata/paths are not validated before
delete/copy. A failed singleton initialization also leaves its instance cached.
These are not silently reproduced as Rust economic behavior.

## Verification and remaining work

The offline oracle contains 41 actual-source observations, including 26 integer
cadence/cleanup/unsafe-selection vectors checked by Rust. Functional tests cover
all three networks, full registry/ledger/spot/replay restart, open orders/fills,
atomic multi-block rollback, branch replay after rollback, invalid mixed-block
nonmutation, independent policy/anchor/tip checks, create-versus-open behavior,
historical-empty-store rejection, semantic corruption with valid journal CRCs,
bounded header retention and durable oldest-floor restart. Private unit tests
inject a real partial synced frame and a full synced commit with lost
acknowledgement; they verify cached-read denial and deliberate reopening.

```text
node scripts/persistence_vectors.cjs
node scripts/persistence_vectors.cjs --source-root /path/to/tradelayer.js
cargo test --test persistence --frozen
cargo test --lib persistence::fault_tests --frozen
```

Use `--write` only for intentional fixture regeneration. Node requires no npm
packages or network access. These source traces and synthetic joint blocks are
not historical chain replay. NeDB query/migration parity, coherent full-economic
state ownership, archival catalogs/backups, authenticated deep-reorg recovery,
compaction, cryptographic state authorization and historical acceptance remain
outstanding roadmap work.
